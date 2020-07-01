use std::collections::HashMap;
use std::{convert::TryFrom, sync::Arc};

use crate::formatting::format_time_since;
use chrono::Utc;
use eyre::{eyre, Result};
use futures::future::{err, ok, Ready};
use glow_events::v2::{Event, Message, Payload};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::value::to_value;
use serde_json::{json, Value};

pub(crate) trait View {
    fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T);
    fn render(&self, template: &str) -> Result<String>;
}

pub struct TeraView {
    tera: Arc<tera::Tera>,
    ctx: tera::Context,
}

impl TeraView {
    pub(crate) fn new(tera: Arc<tera::Tera>) -> Self {
        Self {
            tera,
            ctx: tera::Context::new(),
        }
    }
}

impl actix_web::FromRequest for TeraView {
    type Config = ();
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        if let Some(tmpl) = req.app_data::<actix_web::web::Data<tera::Tera>>() {
            ok(TeraView::new(tmpl.clone().into_inner()))
        } else {
            err(actix_web::error::ErrorInternalServerError(
                "Could not build template view",
            ))
        }
    }
}

impl View for TeraView {
    fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T) {
        self.ctx.insert(key, val)
    }

    fn render(&self, template: &str) -> Result<String> {
        Ok(self.tera.render(template, &self.ctx)?)
    }
}

#[cfg(test)]
struct TestView {
    ctx: HashMap<String, Value>,
}

#[cfg(test)]
impl View for TestView {
    fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T) {
        self.ctx.insert(key.into(), to_value(val).unwrap());
    }

    fn render(&self, template: &str) -> Result<String> {
        Ok(template.to_string())
    }
}

// TODO: consider moving these data types to their own module

#[derive(Debug, Serialize)]
pub(crate) struct Measurement {
    temperature: String,
    humidity: String,
    age: String,
}

impl TryFrom<Message> for Measurement {
    type Error = eyre::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        if let Payload::Event(Event::Measurement(measurement)) = message.payload() {
            Ok(Measurement {
                temperature: format!("{:.2}", measurement.temperature),
                humidity: format!("{:.2}", measurement.humidity),
                age: format_time_since(Utc::now(), message.stamp()),
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
        Event::Measurement(_) => "eco",
        Event::MeasurementFailure => "eco",
        Event::SingleTap => "touch_app",
        Event::Devices(_) => "settings_remote",
        Event::HeaterStarted => "settings_remote",
        Event::HeaterStopped => "settings_remote",
        Event::LEDBrightness(_) => "brightness_4",
        Event::LEDColours(_) => "brightness_4",
        Event::Started => "started",
    }
}

fn get_event_icon_colour(event: &Event) -> &'static str {
    match event {
        Event::Measurement(_) => "green",
        Event::MeasurementFailure => "green",
        Event::SingleTap => "teal",
        Event::Devices(_) => "amber",
        Event::HeaterStarted => "amber",
        Event::HeaterStopped => "amber",
        Event::LEDBrightness(_) => "light-blue",
        Event::LEDColours(_) => "light-blue",
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
