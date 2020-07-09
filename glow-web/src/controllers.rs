use std::convert::TryFrom;

use chrono::{Duration, Utc};
use eyre::{Result, WrapErr};
use itertools::Itertools;

use glow_events::v2::{Command, Event, Message, Payload};

use crate::session::Session;
use crate::store::Store;
use crate::view::data::{ClimateObservation, EventSummary, Measurement};
use crate::view::View;

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
        "climate_history",
        &store
            .get_climate_history_since(Duration::hours(24))?
            .into_iter()
            .map(ClimateObservation::from)
            .group_by(|m| m.date.clone())
            .into_iter()
            .map(|(date, summaries)| (date, summaries.collect()))
            .collect::<Vec<(String, Vec<ClimateObservation>)>>(),
    );

    Ok(view.render("index.html")?)
}

pub(crate) fn set_brightness(
    store: &impl Store,
    session: &mut impl Session,
    brightness: f32,
) -> Result<()> {
    store.queue_command(Command::SetBrightness(brightness))?;
    session.set("flash", "set brightness event was queued")?;

    Ok(())
}

pub(crate) fn list_devices(store: &impl Store, session: &mut impl Session) -> Result<()> {
    store.queue_command(Command::ListDevices)?;
    session.set("flash", "list devices request sent")?;

    Ok(())
}

pub(crate) fn run_heater(store: &impl Store, session: &mut impl Session) -> Result<()> {
    let latest_event = store
        .get_latest_event_like(&r#"{"TPLink":"RunHeater"}"#)
        .wrap_err("failed to get latest heater event")?;

    let can_run_heater = if let Some(latest_event) = latest_event {
        Utc::now()
            .signed_duration_since(latest_event.stamp())
            .num_minutes()
            > 2
    } else {
        true
    };

    if can_run_heater {
        store
            .queue_command(Command::RunHeater)
            .wrap_err("failed to queue run heater event")?;
        session.set("flash", "run heater event queued")?;
    } else {
        session.set("flash", "cannot queue run heater event")?;
    }

    Ok(())
}

pub(crate) fn stop_heater(store: &impl Store, session: &mut impl Session) -> Result<()> {
    store
        .queue_command(Command::StopHeater)
        .wrap_err("failed to queue stop heater event")?;
    session.set("flash", "stop heater event queued")?;

    Ok(())
}

pub(crate) fn stop_device(store: &impl Store, session: &mut impl Session) -> Result<()> {
    store
        .queue_command(Command::Stop)
        .wrap_err("failed to stop device")?;
    session.set("flash", "stop event queued")?;

    Ok(())
}

pub(crate) fn sign_in(
    session: &impl Session,
    password: &str,
    entered_password: &str,
) -> Result<bool> {
    if argon2::verify_encoded(password, entered_password.as_bytes())? {
        session.set("authenticated", true)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn sign_out(session: &impl Session) -> Result<()> {
    session.set("authenticated", false)
}

pub(crate) fn store_events(store: &impl Store, events: &[Message]) -> Result<Vec<Message>> {
    for event in events {
        store.add_event(event).unwrap();
        if let Payload::Event(Event::Measurement(measurement)) = event.payload() {
            store.add_measurement(event.stamp(), measurement).unwrap();
        }
    }
    store.dequeue_commands()
}

pub(crate) fn list_events(store: &impl Store) -> Result<Vec<Message>> {
    store.get_latest_events(20)
}

#[cfg(test)]
mod tests {
    use super::index;

    use crate::session::test::TestSession;
    use crate::store::test::{now, TestDb};
    use crate::{view::data::ClimateObservation, view::test::TestView};
    use chrono::Duration;

    #[test]
    fn index_climate_history() {
        // arrange
        let db = TestDb::with_now(now);
        let store = db.store().unwrap();
        let until = now();
        let since = until - Duration::hours(26);
        TestDb::add_measurements(&store, 1000, since, until).unwrap();
        TestDb::add_observations(&store, 1000, since, until).unwrap();

        // set up database
        let mut session = TestSession::default();
        let mut view = TestView::default();

        // act
        index(&store, &mut view, &mut session).unwrap();

        // assert
        let climate_history: Vec<(String, Vec<ClimateObservation>)> =
            view.get("climate_history").unwrap();

        assert_eq!(climate_history.len(), 2);
        assert_eq!(climate_history[0].1.len() + climate_history[1].1.len(), 25);
        assert!(climate_history
            .iter()
            .all(|(_, observations)| observations.iter().all(|o| &o.time[2..] == ":00")));
    }
}
