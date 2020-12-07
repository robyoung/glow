use std::{net::SocketAddr, time};

use log::{debug, error};
use tokio::stream::StreamExt;
use tplinker::{capabilities::Switch, datatypes::DeviceData, devices::Device, discovery::discover};

use glow_events::{
    v2::{Event, Message},
    TPLinkDevice,
};

use crate::events::Sender;

const HEATER_ON_TIME: time::Duration = time::Duration::from_secs(90);

struct TPLinkDeviceWrap(TPLinkDevice);

impl From<DeviceData> for TPLinkDeviceWrap {
    fn from(device: DeviceData) -> Self {
        TPLinkDeviceWrap(TPLinkDevice {
            name: device.sysinfo().alias.to_owned(),
        })
    }
}

pub async fn handler(tx: Sender) {
    let rx = tx.subscribe();

    tokio::pin! {
        let commands = rx.into_stream()
            .filter(Result::is_ok)
            .map(Result::unwrap)
            .filter_map(Message::into_command);
    }

    debug!("Listening for TPLink commands");

    while let Some(command) = commands.next().await {
        use glow_events::v2::Command::*;
        use glow_events::v2::Event::*;

        match command {
            ListDevices => {
                debug!("Listing TPLink devices");
                match async_discover().await {
                    Ok(result) => {
                        let devices = result
                            .into_iter()
                            .map(|(_addr, device)| TPLinkDeviceWrap::from(device).0)
                            .collect::<Vec<_>>();

                        let message = Message::new_event(Devices(devices));
                        tx.send(message)
                            .expect("failed to write TPLink device list to channel");
                    }
                    Err(err) => error!("Failed to list TPLink devices {}", err),
                }
            }
            command @ RunHeater | command @ StopHeater => {
                debug!("Running or Stopping heater");
                if let Some((addr, data)) = async_find_by_alias(&"Heater").await {
                    let device = Device::from_data(addr, &data);

                    if let Device::HS100(_) = device {
                        match command {
                            RunHeater => async_run_heater(device, &tx).await,
                            StopHeater => async_stop_header(device, &tx).await,
                            _ => unreachable!(),
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

async fn async_discover() -> tplinker::error::Result<Vec<(SocketAddr, DeviceData)>> {
    let (tx, mut rx) = tokio::sync::oneshot::channel();

    tokio::task::spawn_blocking(move || {
        tx.send(discover()).unwrap();
    })
    .await
    .unwrap();

    rx.try_recv().unwrap()
}

async fn async_find_by_alias(alias: &str) -> Option<(SocketAddr, DeviceData)> {
    if let Ok(result) = async_discover().await {
        for (addr, device) in result {
            if device.clone().sysinfo().alias == alias {
                return Some((addr, device));
            }
        }
    }
    None
}

async fn async_run_heater(device: Device, sender: &Sender) {
    if let Device::HS100(inner) = device {
        let inner1 = inner.clone();
        tokio::task::spawn_blocking(move || {
            inner1
                .switch_on()
                .unwrap_or_else(|_err| error!("Failed to switch heater on"));
        })
        .await
        .unwrap_or_else(|_| error!("Failed to spawn tplink switch heater on"));

        sender
            .send(Message::new_event(Event::HeaterStarted))
            .unwrap_or_else(|_err| {
                error!("Failed to write heater on event");
                0
            });

        tokio::time::delay_for(HEATER_ON_TIME).await;

        tokio::task::spawn_blocking(move || {
            inner
                .switch_off()
                .unwrap_or_else(|_err| error!("Failed to switch heater off"));
        })
        .await
        .unwrap_or_else(|_| error!("Failed to spawn tplink switch heater off"));

        sender
            .send(Message::new_event(Event::HeaterStopped))
            .unwrap_or_else(|_err| {
                error!("Failed to write heater off event");
                0
            });
    }
}

async fn async_stop_header(device: Device, sender: &Sender) {
    if let Device::HS100(inner) = device {
        tokio::task::spawn_blocking(move || {
            inner
                .switch_off()
                .unwrap_or_else(|_err| error!("Failed to switch heater off"));
        })
        .await
        .unwrap_or_else(|_| error!("Failed to spawn tplink switch heater off"));

        sender
            .send(Message::new_event(Event::HeaterStopped))
            .unwrap_or_else(|_err| {
                error!("Failed to write heater off event");
                0
            });
    }
}
