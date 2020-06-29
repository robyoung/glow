use std::collections::HashMap;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use glow_events::v2::{Event, Message, Payload};

pub(crate) fn format_time_since(now: DateTime<Utc>, stamp: DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(stamp);
    let mut parts = Vec::new();
    let num_minutes = duration.num_minutes();
    if num_minutes < 60 {
        let word = if num_minutes == 1 {
            "minute"
        } else {
            "minutes"
        };
        if num_minutes > 0 {
            parts.push(format!("{} {}", num_minutes, word));
        }

        let num_seconds = duration.num_seconds() - num_minutes * 60;
        if parts.is_empty() || num_seconds > 0 {
            let word = if num_seconds == 1 {
                "second"
            } else {
                "seconds"
            };

            parts.push(format!("{} {}", num_seconds, word));
        }
    } else if duration.num_days() > 0 {
        parts.push(if duration.num_days() == 1 {
            String::from("more than a day")
        } else {
            format!("more than {} days", duration.num_days())
        });
    } else if duration.num_hours() > 0 {
        parts.push(if duration.num_hours() == 1 {
            String::from("more than an hour")
        } else {
            format!("more than {} hours", duration.num_hours())
        });
    }

    parts.as_slice().join(", ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn time_since_stamp_is_correctly_formatted() {
        let cases = [
            (12, "12 seconds"),
            (1212, "20 minutes, 12 seconds"),
            (12121, "more than 3 hours"),
            (121212, "more than a day"),
            (1212121, "more than 14 days"),
        ];
        for (seconds, formatted) in cases.iter() {
            let now = Utc::now();
            let then = now.checked_sub_signed(Duration::seconds(*seconds)).unwrap();

            assert_eq!(format_time_since(now, then), *formatted,);
        }
    }
}
