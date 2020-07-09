//! Data types for passing to views
//!
//! These data types are fully formatted and ready to be displayed in a UI.
//! They often have a corollary in the `data` module.
use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use glow_events::v2::{Event, Message, Payload};

use crate::data;
use crate::formatting::format_time_since;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClimateMeasurement {
    pub temperature: String,
    pub humidity: String,
}

impl From<data::ClimateMeasurement> for ClimateMeasurement {
    fn from(measurement: data::ClimateMeasurement) -> Self {
        ClimateMeasurement {
            temperature: format!("{:.1}", measurement.temperature),
            humidity: format!("{:.1}", measurement.humidity),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClimateObservation {
    pub indoor: Option<ClimateMeasurement>,
    pub outdoor: Option<ClimateMeasurement>,
    pub age: String,
    pub date: String,
    pub time: String,
}

impl From<data::ClimateObservation> for ClimateObservation {
    fn from(observation: data::ClimateObservation) -> Self {
        let date = observation.date_time.format("%Y-%m-%d").to_string();
        let time = observation.date_time.format("%H:%M").to_string();
        let age = format_time_since(Utc::now(), observation.date_time);
        Self {
            indoor: observation.indoor.map(ClimateMeasurement::from),
            outdoor: observation.outdoor.map(ClimateMeasurement::from),
            age,
            date,
            time,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct EventSummary {
    pub icon: String,
    pub icon_colour: String,
    pub stamp: String,
    pub date: String,
    pub time: String,
    pub title: String,
    pub detail: String,
    pub event_type: String,
    pub extra: HashMap<String, Value>,
}

impl From<Message> for EventSummary {
    fn from(message: Message) -> Self {
        EventSummary::from(&message)
    }
}

impl From<&Message> for EventSummary {
    fn from(message: &Message) -> Self {
        let mut summary = EventSummary::default();

        if let Payload::Event(event) = message.payload() {
            summary.stamp = message.stamp().format("%F %T").to_string();
            summary.date = message.stamp().format("%Y-%m-%d").to_string();
            summary.time = message.stamp().format("%H:%M:%S").to_string();
            summary.title = event.title().to_string();
            summary.icon = get_event_icon(event).to_string();
            summary.icon_colour = get_event_icon_colour(event).to_string();
            summary.detail = format!("{}", event);
            summary.event_type = event.event_type().to_string();
            summary.extra = get_event_extra(event);
        }
        summary
    }
}

fn get_event_icon(event: &Event) -> &'static str {
    match event {
        Event::Measurement(_) | Event::MeasurementFailure => "eco",
        Event::SingleTap => "touch_app",
        Event::Devices(_) | Event::HeaterStarted | Event::HeaterStopped => "settings_remote",
        Event::LEDBrightness(_) | Event::LEDColours(_) => "brightness_4",
        Event::Started => "started",
    }
}

fn get_event_icon_colour(event: &Event) -> &'static str {
    match event {
        Event::Measurement(_) | Event::MeasurementFailure => "green",
        Event::SingleTap => "teal",
        Event::Devices(_) | Event::HeaterStarted | Event::HeaterStopped => "amber",
        Event::LEDBrightness(_) | Event::LEDColours(_) => "light-blue",
        Event::Started => "red",
    }
}

fn get_event_extra(event: &Event) -> HashMap<String, Value> {
    let mut extra = HashMap::new();
    match event {
        Event::LEDColours(colours) => {
            let colours = colours
                .iter()
                .map(|c| format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2))
                .collect::<Vec<String>>();

            extra.insert("colours".into(), colours.into());
        }
        Event::Devices(devices) => {
            let devices = devices
                .iter()
                .map(|d| json!({"name": d.name }))
                .collect::<Vec<Value>>();

            extra.insert("devices".into(), devices.into());
        }
        _ => {}
    }
    extra
}

#[derive(Deserialize)]
pub struct SetBrightness {
    pub brightness: u32,
}

#[derive(Deserialize)]
pub struct Login {
    pub password: String,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::{json, value::Value};

    use glow_events::{
        v2::{Command, Event, Message, Payload},
        Measurement, TPLinkDevice,
    };

    use super::EventSummary;

    #[test]
    fn event_summary() {
        struct EventSummaryTest {
            message: Message,
            detail: &'static str,
            icon: &'static str,
            icon_colour: &'static str,
            extra: HashMap<String, Value>,
        }
        impl EventSummaryTest {
            fn new(
                message: Message,
                detail: &'static str,
                icon: &'static str,
                icon_colour: &'static str,
                extra: HashMap<String, Value>,
            ) -> Self {
                Self {
                    message,
                    detail,
                    icon,
                    icon_colour,
                    extra,
                }
            }
        }
        let messages = vec![
            EventSummaryTest::new(
                Message::new(Payload::Event(Event::Measurement(Measurement::new(
                    1.1, 2.2,
                )))),
                "temperature: 1.10°C humidity: 2.20%",
                "eco",
                "green",
                HashMap::new(),
            ),
            EventSummaryTest::new(
                Message::new(Payload::Event(Event::SingleTap)),
                "single tap",
                "touch_app",
                "teal",
                HashMap::new(),
            ),
            EventSummaryTest::new(
                Message::new(Payload::Event(Event::Started)),
                "started",
                "started",
                "red",
                HashMap::new(),
            ),
            EventSummaryTest::new(
                Message::new(Payload::Event(Event::LEDColours(vec![
                    (123, 123, 123),
                    (123, 123, 123),
                    (123, 123, 123),
                ]))),
                "colours updated",
                "brightness_4",
                "light-blue",
                [(
                    String::from("colours"),
                    json!([
                        "#7B7B7B".to_string(),
                        "#7B7B7B".to_string(),
                        "#7B7B7B".to_string()
                    ]),
                )]
                .iter()
                .cloned()
                .collect(),
            ),
            EventSummaryTest::new(
                Message::new(Payload::Event(Event::Devices(vec![TPLinkDevice {
                    name: "plug".to_string(),
                }]))),
                "device list",
                "settings_remote",
                "amber",
                [("devices".to_string(), json!([{"name": "plug".to_string()}]))]
                    .iter()
                    .cloned()
                    .collect(),
            ),
            EventSummaryTest::new(
                Message::new(Payload::Command(Command::Stop)),
                "",
                "",
                "",
                HashMap::new(),
            ),
        ];

        for message in messages {
            let summary = EventSummary::from(message.message);

            assert_eq!(summary.detail, message.detail);
            assert_eq!(summary.icon, message.icon);
            assert_eq!(summary.icon_colour, message.icon_colour);
            assert_eq!(summary.extra, message.extra);
        }
    }
}
