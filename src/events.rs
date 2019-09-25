use chrono::{offset::Utc, DateTime};
use std::sync::mpsc::{SyncSender, sync_channel};

#[derive(Debug, Clone)]
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

#[derive(Debug, PartialEq, Clone)]
pub enum Message {
    Environment(Measurement),
    TapEvent,
    UpdateLEDs,
    LEDParty,
    Stop,
}

#[derive(Debug, PartialEq, Clone)]
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
        if let Message::Stop = event.message() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn new_event_has_recent_timestamp() {
        // act
        let event = Event::new(Message::TapEvent);

        // assert
        let diff = Utc::now() - event.stamp();
        assert!(diff.to_std().unwrap() < Duration::from_secs(1));
    }

    #[test]
    fn new_environment_event() {
        // act
        let event = Event::new_envirornment(12.12, 13.13);

        // assert
        assert_eq!(*event.message(), Message::Environment(Measurement::new(12.12, 13.13)));
    }

    fn send_one_source(sender: SyncSender<Event>) {
        sender.send(Event::new(Message::TapEvent)).unwrap();
        sender.send(Event::new(Message::Stop)).unwrap();
    }

    struct StoringEventReceiver {
        events: SyncSender<Event>
    }

    impl EventHandler for StoringEventReceiver {
        fn handle(&mut self, event: &Event, _: &SyncSender<Event>) {
            self.events.send(event.clone()).unwrap();
        }
    }

    #[test]
    fn run_run_loop() {
        // arrange
        let (sender, receiver) = sync_channel(20);
        let handler = StoringEventReceiver { events: sender };

        // act
        run_loop(vec![send_one_source], vec![Box::new(handler)]);

        // assert
        let events = receiver.iter().collect::<Vec<Event>>();

        assert_eq!(events.len(), 2);
        assert_eq!(*events[0].message(), Message::TapEvent);
        assert_eq!(*events[1].message(), Message::Stop);
    }
}
