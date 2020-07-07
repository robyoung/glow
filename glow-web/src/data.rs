use std::{collections::HashMap, convert::TryFrom};

use chrono::Utc;
use eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use glow_events::v2::{Event, Message, Payload};

use crate::formatting::format_time_since;

pub struct AppData {
    pub token: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct Measurement {
    temperature: String,
    humidity: String,
    age: String,
    date: String,
    time: String,
}

impl TryFrom<Message> for Measurement {
    type Error = eyre::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        if let Payload::Event(Event::Measurement(measurement)) = message.payload() {
            Ok(Measurement {
                temperature: format!("{:.2}", measurement.temperature),
                humidity: format!("{:.2}", measurement.humidity),
                age: format_time_since(Utc::now(), message.stamp()),
                date: message.stamp().format("%Y-%m-%d").to_string(),
                time: message.stamp().format("%H:%M:%S").to_string(),
            })
        } else {
            Err(eyre!("not a measurement"))
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
