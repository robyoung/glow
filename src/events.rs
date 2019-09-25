use chrono::{offset::Utc, DateTime};
use std::sync::mpsc::{SyncSender, sync_channel};

#[derive(Debug)]
pub struct Event {
    stamp: DateTime<Utc>,
    message: Message,
}

impl Event {
    pub fn new(message: Message) -> Event {
        Self {
            stamp: Utc::now(),
            message,
        }
    }

    pub fn new_envirornment(temperature: f64, humidity: f64) -> Event {
        Self::new(Message::Environment(Measurement::new(temperature, humidity)))
    }

    pub fn stamp(&self) -> DateTime<Utc> {
        self.stamp
    }

    pub fn message(&self) -> &Message {
        &self.message
    }
}

#[derive(Debug)]
pub enum Message {
    Environment(Measurement),
    TapEvent,
    UpdateLEDs,
    LEDParty,
}

#[derive(Debug)]
pub struct Measurement {
    pub temperature: f64,
    pub humidity: f64,
}

impl Measurement {
    pub fn new(temperature: f64, humidity: f64) -> Self {
        Self { temperature, humidity }
    }
}

pub type EventSource = fn(SyncSender<Event>);

pub trait EventHandler {
    fn handle(&mut self, event: &Event, sender: &SyncSender<Event>);
}

pub fn run_loop(sources: Vec<EventSource>, mut handlers: Vec<Box<dyn EventHandler>>) {
    let (sender, receiver) = sync_channel(20);

    for source in sources {
        source(sender.clone());
    }

    for event in receiver.iter() {
        for handler in handlers.iter_mut() {
            handler.handle(&event, &sender);
        }
    }
}
