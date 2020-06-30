use std::convert::TryFrom;

use chrono::Timelike;
use eyre::{eyre, Result, WrapErr};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Serialize};

use glow_events::v2::Message;

use crate::store::Store;
use crate::view::{EventSummary, Measurement, View};

pub(crate) fn index(
    store: &impl Store,
    view: &mut impl View,
    session: &mut impl Session,
) -> Result<String> {
    view.insert("flash", &session.pop::<Option<String>>("flash")?);

    if let Some(message) = store.get_latest_measurement() {
        if let Ok(measurement) = Measurement::try_from(message) {
            view.insert("measurement", &measurement);
        }
    }

    view.insert(
        "events",
        &store
            .get_latest_events(20)
            .unwrap_or_default()
            .iter()
            .map(EventSummary::from)
            .collect::<Vec<EventSummary>>(),
    );

    view.insert(
        "measurements",
        &store
            .get_measurements_since(chrono::Duration::hours(24))
            .wrap_err("failed getting measurements")?
            .iter()
            .group_by(|event| event.stamp().hour())
            .into_iter()
            .map(|(_, group)| {
                let event = group.last().unwrap();
                Message::raw(event.stamp(), event.payload().clone())
            })
            .map(EventSummary::from)
            .collect::<Vec<EventSummary>>(),
    );

    Ok(view.render("index.html")?)
}

pub(crate) trait Session {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    fn set<T: Serialize>(&self, key: &str, value: T) -> Result<()>;
    fn pop<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>>;
    fn remove(&mut self, key: &str);
}

pub(crate) struct ActixSession(actix_session::Session);

impl ActixSession {
    pub fn new(session: actix_session::Session) -> Self {
        ActixSession(session)
    }
}

impl Session for ActixSession {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        Ok(self
            .0
            .get(key)
            .map_err(|e| eyre!("failed to de-serialize session: {}", e))?)
    }

    fn set<T: Serialize>(&self, key: &str, value: T) -> Result<()> {
        Ok(self
            .0
            .set(key, value)
            .map_err(|e| eyre!("failed to serialize session: {}", e))?)
    }

    fn pop<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>> {
        Ok(if let Some(value) = self.get(key)? {
            self.remove(key);
            Some(value)
        } else {
            None
        })
    }

    fn remove(&mut self, key: &str) {
        self.0.remove(key)
    }
}

#[cfg(test)]
mod tests {}
