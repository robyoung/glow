use std::{net::SocketAddr, sync::mpsc::SyncSender, thread, time};

use log::error;
use tplinker::{capabilities::Switch, datatypes::DeviceData, devices::Device, discovery::discover};

use glow_events::{
    v2::{Command, Event, Message, Payload},
    TPLinkDevice,
};

use crate::events::MessageHandler;

const HEATER_ON_TIME: time::Duration = time::Duration::from_secs(90);

pub struct TPLinkHandler {}

impl MessageHandler for TPLinkHandler {
    fn handle(&mut self, message: &Message, sender: &SyncSender<Message>) {
        match message.payload() {
            Payload::Command(Command::ListDevices) => match discover() {
                Ok(result) => {
                    let mut devices: Vec<TPLinkDevice> = Vec::new();
                    for (_addr, device) in result {
                        devices.push(TPLinkDevice {
                            name: device.sysinfo().alias.clone(),
                        })
                    }
                    let message = Message::event(Event::Devices(devices));
                    if let Err(err) = sender.send(message) {
                        error!("Failed to write TPLink device list to channel: {:?}", err);
                    }
                }
                Err(err) => error!("Failed to list TPLink devices {}", err),
            },
            Payload::Command(Command::RunHeater) => {
                if let Some((addr, data)) = find_by_alias(&"Heater") {
                    let device = Device::from_data(addr, &data);

                    if let Device::HS100(device) = device {
                        device
                            .switch_on()
                            .unwrap_or_else(|_err| error!("Failed to switch heater on"));
                        sender
                            .send(Message::event(Event::HeaterStarted))
                            .unwrap_or_else(|_err| error!("Failed to write heater on event"));
                        let sender = sender.clone();
                        thread::spawn(move || {
                            thread::sleep(HEATER_ON_TIME);
                            // TODO: retry on failure
                            device
                                .switch_off()
                                .unwrap_or_else(|_err| error!("Failed to switch heater off"));
                            sender
                                .send(Message::event(Event::HeaterStopped))
                                .unwrap_or_else(|_err| {
                                    error!("Failed to write heater off event")
                                });
                        });
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
