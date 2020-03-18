use std::{net::SocketAddr, sync::mpsc::SyncSender, thread, time};

use log::error;
use tplinker::{
    capabilities::Switch,
    datatypes::DeviceData,
    devices::Device,
    discovery::discover,
};

use glow_events::{Event, Message, TPLinkDevice, TPLinkEvent};

use crate::events::EventHandler;

const HEATER_ON_TIME: time::Duration = time::Duration::from_secs(90);

pub struct TPLinkHandler {}

impl EventHandler for TPLinkHandler {
    fn handle(&mut self, event: &Event, sender: &SyncSender<Event>) {
        match event.message() {
            Message::TPLink(TPLinkEvent::ListDevices) => match discover() {
                Ok(result) => {
                    let mut devices: Vec<TPLinkDevice> = Vec::new();
                    for (_addr, device) in result {
                        devices.push(TPLinkDevice {
                            name: device.sysinfo().alias.clone(),
                        })
                    }
                    let event = Event::new(Message::TPLink(TPLinkEvent::DeviceList(devices)));
                    if let Err(err) = sender.send(event) {
                        error!("Failed to write TPLink device list to channel: {:?}", err);
                    }
                }
                Err(err) => error!("Failed to list TPLink devices {}", err),
            },
            Message::TPLink(TPLinkEvent::RunHeater) => {
                if let Some((addr, data)) = find_by_alias(&"Heater") {
                    let device = Device::from_data(addr, &data);

                    match device {
                        Device::HS100(device) => {
                            device
                                .switch_on()
                                .unwrap_or_else(|_err| error!("Failed to switch heater on"));
                            sender
                                .send(Event::new(Message::TPLink(TPLinkEvent::HeaterStarted)))
                                .unwrap_or_else(|_err| error!("Failed to write heater on event"));
                            let sender = sender.clone();
                            thread::spawn(move || {
                                thread::sleep(HEATER_ON_TIME);
                                // TODO: retry on failure
                                device
                                    .switch_off()
                                    .unwrap_or_else(|_err| error!("Failed to switch heater off"));
                                sender
                                    .send(Event::new(Message::TPLink(TPLinkEvent::HeaterStopped)))
                                    .unwrap_or_else(|_err| {
                                        error!("Failed to write heater off event")
                                    });
                            });
                        }
                        _ => {}
                    }
                } else {
                    error!("Heater not found");
                }
            }
            _ => {}
        }
    }
}

fn find_by_alias(alias: &str) -> Option<(SocketAddr, DeviceData)> {
    if let Ok(result) = discover() {
        for (addr, device) in result {
            if device.clone().sysinfo().alias == alias {
                return Some((addr, device));
            }
        }
    }
    None
}
