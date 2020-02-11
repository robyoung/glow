use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};

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

    pub fn raw(stamp: DateTime<Utc>, message: Message) -> Self {
        Self { stamp, message }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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

    #[test]
    fn serialize_a_message() {
        // arrange
        let message =
            Message::Environment(EnvironmentEvent::Measurement(Measurement::new(12.3, 43.1)));

        // act
        let message_str = serde_json::to_string(&message);

        // assert
        assert_eq!(
            message_str.unwrap(),
            r#"{"Environment":{"Measurement":{"temperature":12.3,"humidity":43.1}}}"#
        );
    }

    #[test]
    fn serialize_a_stop_message() {
        // arrange
        let message = Message::Stop;

        // act
        let message_str = serde_json::to_string(&message);

        // assert
        assert_eq!(message_str.unwrap(), r#""Stop""#);
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
