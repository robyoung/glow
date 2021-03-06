use std::fmt;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};

use crate::{Measurement, TPLinkDevice};

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

    pub fn measurement(&self) -> Option<Measurement> {
        if let Message::Environment(EnvironmentEvent::Measurement(measurement)) = self.message {
            Some(measurement)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Message {
    Environment(EnvironmentEvent),
    Tap(TapEvent),
    TPLink(TPLinkEvent),
    LED(LEDEvent),
    Stop,
    Started,
}

impl Message {
    pub fn title(&self) -> String {
        match self {
            Message::Environment(_) => String::from("Environment event"),
            Message::Tap(_) => String::from("Tap event"),
            Message::TPLink(_) => String::from("TP-Link event"),
            Message::LED(_) => String::from("LED event"),
            Message::Stop => String::from("Stop event"),
            Message::Started => String::from("Device started"),
        }
    }

    pub fn event_type(&self) -> String {
        match self {
            Message::Environment(event) => format!("environment.{}", event.event_type()),
            Message::Tap(event) => format!("tap.{}", event.event_type()),
            Message::TPLink(event) => format!("tplink.{}", event.event_type()),
            Message::LED(event) => format!("led.{}", event.event_type()),
            Message::Stop => String::from("stop"),
            Message::Started => String::from("started"),
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::Environment(event) => write!(f, "{}", event),
            Message::Tap(event) => write!(f, "{}", event),
            Message::TPLink(event) => write!(f, "{}", event),
            Message::LED(event) => write!(f, "{}", event),
            Message::Stop => write!(f, "stop"),
            Message::Started => write!(f, "start"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum EnvironmentEvent {
    Measurement(Measurement),
    Failure,
}

impl EnvironmentEvent {
    fn event_type(&self) -> String {
        match self {
            EnvironmentEvent::Measurement(_) => String::from("measurement"),
            EnvironmentEvent::Failure => String::from("failure"),
        }
    }
}

impl fmt::Display for EnvironmentEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvironmentEvent::Measurement(measurement) => write!(
                f,
                "temperature: {}°C humidity: {}%",
                measurement.temperature, measurement.humidity
            ),
            EnvironmentEvent::Failure => write!(f, "failure"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum TapEvent {
    SingleTap,
}

impl TapEvent {
    fn event_type(&self) -> String {
        match self {
            TapEvent::SingleTap => String::from("single-tap"),
        }
    }
}

impl fmt::Display for TapEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "single tap")
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum TPLinkEvent {
    ListDevices,
    DeviceList(Vec<TPLinkDevice>),
    RunHeater,
    HeaterStarted,
    HeaterStopped,
}

impl TPLinkEvent {
    fn event_type(&self) -> String {
        match self {
            TPLinkEvent::ListDevices => String::from("list-devices"),
            TPLinkEvent::DeviceList(_) => String::from("device-list"),
            TPLinkEvent::RunHeater => String::from("run-heater"),
            TPLinkEvent::HeaterStarted => String::from("heater-started"),
            TPLinkEvent::HeaterStopped => String::from("heater-stopped"),
        }
    }
}

impl fmt::Display for TPLinkEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TPLinkEvent::ListDevices => write!(f, "list devices"),
            TPLinkEvent::DeviceList(_) => write!(f, "device list"),
            TPLinkEvent::RunHeater => write!(f, "run heater"),
            TPLinkEvent::HeaterStarted => write!(f, "heater started"),
            TPLinkEvent::HeaterStopped => write!(f, "heater stopped"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum LEDEvent {
    Brightness(f32),
    Party,
    Update,
    LEDsUpdated(Vec<(u8, u8, u8)>),
}

impl LEDEvent {
    fn event_type(&self) -> String {
        match self {
            LEDEvent::Brightness(_) => String::from("brightness"),
            LEDEvent::Party => String::from("party"),
            LEDEvent::Update => String::from("update"),
            LEDEvent::LEDsUpdated(_) => String::from("leds-updated"),
        }
    }
}

impl fmt::Display for LEDEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LEDEvent::Brightness(brightness) => write!(f, "set brightness to {}", brightness),
            LEDEvent::Party => write!(f, "party mode!"),
            LEDEvent::Update => write!(f, "update LEDs"),
            LEDEvent::LEDsUpdated(_) => write!(f, "LEDs updated"),
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
