use std::fmt;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};

use crate::{Measurement, TPLinkDevice};

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    stamp: DateTime<Utc>,
    payload: Payload,
}

impl Message {
    pub fn new(payload: Payload) -> Self {
        Self::raw(Utc::now(), payload)
    }

    pub fn raw(stamp: DateTime<Utc>, payload: Payload) -> Self {
        Self { stamp, payload }
    }

    pub fn new_command(command: Command) -> Self {
        Self::new(Payload::Command(command))
    }

    pub fn new_event(event: Event) -> Self {
        Self::new(Payload::Event(event))
    }

    pub fn stamp(&self) -> DateTime<Utc> {
        self.stamp
    }

    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    pub fn into_command(self) -> Option<Command> {
        if let Payload::Command(command) = self.payload {
            Some(command)
        } else {
            None
        }
    }

    pub fn into_event(self) -> Option<Event> {
        if let Payload::Event(event) = self.payload {
            Some(event)
        } else {
            None
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    Command(Command),
    Event(Event),
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    ListDevices,
    RunHeater,
    StopHeater,
    SetBrightness(f32),
    UpdateLEDs,
    RunParty,
    Stop,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Measurement(Measurement),
    MeasurementFailure,
    SingleTap,
    Devices(Vec<TPLinkDevice>),
    HeaterStarted,
    HeaterStopped,
    LEDBrightness(f32),
    LEDColours(Vec<(u8, u8, u8)>),
    Started,
}

impl Event {
    pub fn title(&self) -> &'static str {
        match self {
            Event::Measurement(_) => "Measurement event",
            Event::MeasurementFailure => "Measurement failure",
            Event::SingleTap => "Single tap",
            Event::Devices(_) => "Device list",
            Event::HeaterStarted => "Heater started",
            Event::HeaterStopped => "Heater stopped",
            Event::LEDBrightness(_) => "LED brightness",
            Event::LEDColours(_) => "LED colours",
            Event::Started => "Started",
        }
    }

    pub fn event_type(&self) -> &'static str {
        match self {
            Event::Measurement(_) => "environment.measurement",
            Event::MeasurementFailure => "environment.failure",
            Event::SingleTap => "tap.single",
            Event::Devices(_) => "tplink.device-list",
            Event::HeaterStarted => "tplink.heater-started",
            Event::HeaterStopped => "tplink.heater-stopped",
            Event::LEDBrightness(_) => "led.brightness",
            Event::LEDColours(_) => "led.colours",
            Event::Started => "started",
        }
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Measurement(measurement) => write!(
                f,
                "temperature: {:.2}°C humidity: {:.2}%",
                measurement.temperature, measurement.humidity
            ),
            Event::MeasurementFailure => write!(f, "failure"),
            Event::SingleTap => write!(f, "single tap"),
            Event::Devices(_) => write!(f, "device list"),
            Event::HeaterStarted => write!(f, "heater started"),
            Event::HeaterStopped => write!(f, "heater stopped"),
            Event::LEDBrightness(brightness) => write!(f, "brightness: {:.2}", brightness),
            Event::LEDColours(_) => write!(f, "colours updated"),
            Event::Started => write!(f, "started"),
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
        let message = Message::new_event(Event::SingleTap);

        // assert
        let diff = Utc::now() - message.stamp();
        assert!(diff.to_std().unwrap() < Duration::from_secs(1));
    }

    #[test]
    fn new_environment_event() {
        // act
        let message = Message::new_event(Event::Measurement(Measurement::new(12.12, 13.13)));

        // assert
        assert_eq!(
            *message.payload(),
            Payload::Event(Event::Measurement(Measurement::new(12.12, 13.13)))
        );
    }

    #[test]
    fn serialize_a_message() {
        // arrange
        let payload = Payload::Event(Event::Measurement(Measurement::new(12.3, 43.1)));

        // act
        let payload_str = serde_json::to_string(&payload).unwrap();

        // assert
        assert_eq!(
            payload_str,
            r#"{"Event":{"Measurement":{"temperature":12.3,"humidity":43.1}}}"#
        );
    }

    #[test]
    fn serialize_a_stop_message() {
        // arrange
        let command = Command::Stop;

        // act
        let command_str = serde_json::to_string(&command).unwrap();

        // assert
        assert_eq!(command_str, r#""Stop""#);
    }

    #[test]
    fn serialize_deserialize_an_event() {
        // arrange
        let message = Message::new_event(Event::Measurement(Measurement::new(12.12, 13.13)));

        // act
        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&serialized).unwrap();

        // assert
        assert_eq!(message, deserialized);
    }
}
