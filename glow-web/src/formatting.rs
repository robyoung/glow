use std::collections::HashMap;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use glow_events::{Event, LEDEvent, Message, TPLinkEvent};

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
    pub title: String,
    pub detail: String,
    pub event_type: String,
    pub extra: HashMap<String, Value>,
}

impl From<&Event> for EventSummary {
    fn from(event: &Event) -> Self {
        let mut summary = EventSummary::default();

        summary.stamp = format!("{}", event.stamp().format("%F %T"));
        summary.title = event.message().title();
        summary.icon = get_message_icon(event.message());
        summary.icon_colour = get_message_icon_colour(event.message());
        summary.detail = format!("{}", event.message());
        summary.event_type = event.message().event_type();
        summary.extra = get_message_extra(event.message());
        summary
    }
}

fn get_message_icon(message: &Message) -> String {
    match message {
        Message::Environment(_) => "eco",
        Message::Tap(_) => "touch_app",
        Message::TPLink(_) => "settings_remote",
        Message::LED(_) => "brightness_4",
        Message::Stop => "stop",
    }
    .to_string()
}

fn get_message_icon_colour(message: &Message) -> String {
    match message {
        Message::Environment(_) => "green",
        Message::Tap(_) => "teal",
        Message::TPLink(_) => "amber",
        Message::LED(_) => "light-blue",
        Message::Stop => "red",
    }
    .to_string()
}

fn get_message_extra(message: &Message) -> HashMap<String, Value> {
    let mut extra = HashMap::new();
    match message {
        Message::LED(LEDEvent::LEDsUpdated(colours)) => {
            let colours = colours
                .iter()
                .map(|c| format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2))
                .collect::<Vec<String>>();

            extra.insert("colours".into(), colours.into());
        }
        Message::TPLink(TPLinkEvent::DeviceList(devices)) => {
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
