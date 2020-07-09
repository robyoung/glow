//! Data types for use between modules
//!
//! These data types are useful to multiple modules, either because they are
//! core data types (like `AppData`) or because they generalise more specific
//! data types (like `ClimateObservation` and `ClimateMeasurement`).
use core::convert::TryFrom;

use eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

use glow_events::v2::{Event, Message, Payload};

use crate::weather::Observation;
use chrono::{DateTime, Utc};

pub struct AppData {
    pub token: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClimateMeasurement {
    pub temperature: f64,
    pub humidity: f64,
}

impl From<Observation> for ClimateMeasurement {
    fn from(observation: Observation) -> Self {
        ClimateMeasurement {
            temperature: f64::from(observation.temperature),
            humidity: f64::from(observation.humidity),
        }
    }
}

impl TryFrom<Message> for ClimateMeasurement {
    type Error = eyre::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        if let Payload::Event(Event::Measurement(measurement)) = message.payload() {
            Ok(ClimateMeasurement {
                temperature: measurement.temperature as f64,
                humidity: measurement.humidity as f64,
            })
        } else {
            Err(eyre!("not a measurement"))
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClimateObservation {
    pub indoor: Option<ClimateMeasurement>,
    pub outdoor: Option<ClimateMeasurement>,
    pub date_time: DateTime<Utc>,
}

impl ClimateObservation {
    pub fn try_from_parts(
        message: Option<Message>,
        observation: Option<Observation>,
    ) -> Result<Self> {
        // TODO: can this be tidied up?
        let date_time = if message.is_some() {
            message.clone().unwrap().stamp()
        } else if observation.is_some() {
            observation.clone().unwrap().date_time
        } else {
            return Err(eyre!("need at least measurement or observation to be Some"));
        };
        Ok(Self {
            indoor: message.map(ClimateMeasurement::try_from).transpose()?,
            outdoor: observation.map(ClimateMeasurement::from),
            date_time,
        })
    }
}

impl TryFrom<Message> for ClimateObservation {
    type Error = eyre::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let date_time = message.stamp();
        Ok(Self {
            indoor: Some(ClimateMeasurement::try_from(message)?),
            outdoor: None,
            date_time,
        })
    }
}
