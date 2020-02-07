use std::sync::mpsc::{sync_channel, SyncSender};

use chrono::{offset::Utc, DateTime};
use serde::{Serialize, Deserialize};

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
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

    pub fn new_measurement(temperature: f64, humidity: f64) -> Event {
        Self::new(Message::Environment(EnvironmentEvent::Measurement(
            Measurement::new(temperature, humidity),
        )))
    }

    pub fn stamp(&self) -> DateTime<Utc> {
        self.stamp
    }

    pub fn message(&self) -> &Message {
        &self.message
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Message {
    Environment(EnvironmentEvent),
    Tap(TapEvent),
    TPLink(TPLinkEvent),
    LED(LEDEvent),
    Stop,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum EnvironmentEvent {
    Measurement(Measurement),
    Failure,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum TapEvent {
    SingleTap,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum TPLinkEvent {
    ListDevices,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum LEDEvent {
    SetBrightness(u32),
    Party,
    Update,
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct Measurement {
    pub temperature: f64,
    pub humidity: f64,
}

impl Measurement {
    pub fn new(temperature: f64, humidity: f64) -> Self {
        Self {
            temperature,
            humidity,
        }
    }

    pub fn roughly_equal(&self, other: &Measurement) -> bool {
        (self.temperature - other.temperature).abs() < 0.001
            && (self.humidity - other.humidity).abs() < 0.001
    }
}

pub trait EventHandler {
    fn start(&mut self, _sender: SyncSender<Event>) {}
    fn handle(&mut self, _event: &Event, _sender: &SyncSender<Event>) {}
}

pub fn run_loop(mut handlers: Vec<Box<dyn EventHandler>>) {
    let (sender, receiver) = sync_channel(20);

    for handler in handlers.iter_mut() {
        handler.start(sender.clone());
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
    use serde_json;

    #[test]
    fn new_event_has_recent_timestamp() {
        // act
        let event = Event::new(Message::Tap(TapEvent::SingleTap));

        // assert
        let diff = Utc::now() - event.stamp();
        assert!(diff.to_std().unwrap() < Duration::from_secs(1));
    }

    #[test]
    fn new_environment_event() {
        // act
        let event = Event::new_measurement(12.12, 13.13);

        // assert
        assert_eq!(
            *event.message(),
            Message::Environment(EnvironmentEvent::Measurement(Measurement::new(
                12.12, 13.13
            )))
        );
    }

    struct SendOneSource {}

    impl EventHandler for SendOneSource {
        fn start(&mut self, sender: SyncSender<Event>) {
            sender
                .send(Event::new(Message::Tap(TapEvent::SingleTap)))
                .unwrap();
            sender.send(Event::new(Message::Stop)).unwrap();
        }
    }

    struct StoringEventReceiver {
        events: SyncSender<Event>,
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
        run_loop(vec![Box::new(SendOneSource {}), Box::new(handler)]);

        // assert
        let events = receiver.iter().collect::<Vec<Event>>();

        assert_eq!(events.len(), 2);
        assert_eq!(*events[0].message(), Message::Tap(TapEvent::SingleTap));
        assert_eq!(*events[1].message(), Message::Stop);
    }

    #[test]
    fn serialize_a_message() {
        // arrange
        let message = Message::Environment(EnvironmentEvent::Measurement(Measurement::new(12.3, 43.1)));

        // act
        let message_str = serde_json::to_string(&message);

        // assert
        assert_eq!(message_str.unwrap(), r#"{"Environment":{"Measurement":{"temperature":12.3,"humidity":43.1}}}"#);
    }
     #[test]
    fn serialize_deserialize_an_event() {
        // arrange
        let event = Event::new_measurement(12.3, 43.1);

        // act
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&serialized).unwrap();

        // assert
        assert_eq!(event, deserialized);
    }
}
