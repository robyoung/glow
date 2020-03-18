use std::sync::mpsc::SyncSender;

use log::error;
use tplinker::discovery::discover;

use glow_events::{Event, Message, TPLinkDevice, TPLinkEvent};

use crate::events::EventHandler;

pub struct TPLinkHandler {}

impl EventHandler for TPLinkHandler {
    fn handle(&mut self, event: &Event, sender: &SyncSender<Event>) {
        match event.message() {
            Message::TPLink(TPLinkEvent::ListDevices) => match discover() {
                Ok(result) => {
                    let mut devices: Vec<TPLinkDevice> = Vec::new();
                    for (_addr, device) in result {
                        devices.push(TPLinkDevice {
                            name: device.sysinfo().alias,
                        })
                    }
                    let event = Event::new(Message::TPLink(TPLinkEvent::DeviceList(devices)));
                    if let Err(err) = sender.send(event) {
                        error!("Failed to write TPLink device list to channel: {:?}", err);
                    }
                }
                Err(err) => error!("Failed to list TPLink devices {}", err),
            },
            _ => {}
        }
    }
}
