use chrono::{offset::Utc, DateTime};
use fallible_iterator::FallibleIterator;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::{self, SqliteConnectionManager};
use rusqlite::{types::FromSqlError, Result, Row, NO_PARAMS};
use rand::Rng;

use glow_events::{EnvironmentEvent, Event, Measurement, Message};

pub fn setup_db(db_path: &str) -> Pool<SqliteConnectionManager> {
    let pool = Pool::new(SqliteConnectionManager::file(db_path)).unwrap();
    migrate_db(&pool);
    pool
}

fn migrate_db(pool: &Pool<SqliteConnectionManager>) {
    let conn = pool.get().unwrap();
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            stamp DATETIME,
            message TEXT
        );
        "#,
        params![],
    )
    .expect("Cannot create events table");
    conn.execute(
        "CREATE INDEX IF NOT EXISTS events_stamp ON events (stamp);",
        params![],
    )
    .expect("Cannot create events.stamp index");

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS environment_measurements (
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            stamp DATETIME,
            temperature REAL,
            humidity REAL
        );
        "#,
        params![],
    )
    .expect("Cannot create environment_measurements table");
    conn.execute(
        "CREATE INDEX IF NOT EXISTS environment_measurements_stamp ON environment_measurements (stamp);",
        params![],
    )
    .expect("Cannot create events.stamp index");

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS event_queue (
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            stamp DATETIME,
            message TEXT,
            group_token INT DEFAULT 0
        );
        "#,
        params![],
    )
    .expect("Cannot create event_queue table");
    conn.execute(
        "CREATE INDEX IF NOT EXISTS event_queue_created_at ON event_queue (stamp, group_token);",
        params![],
    )
    .expect("Cannot create events.stamp index");
    conn.execute(
        "CREATE INDEX IF NOT EXISTS event_queue_group_token ON event_queue (group_token);",
        params![],
    )
    .expect("Cannot create events.group_token index");
}

pub(crate) fn insert_event(
    conn: &PooledConnection<SqliteConnectionManager>,
    event: &Event,
) -> Result<usize> {
    insert_event_to(&"events", conn, event)
}

pub(crate) fn get_latest_events(
    conn: &PooledConnection<SqliteConnectionManager>,
    limit: u32,
) -> Result<Vec<Event>> {
    conn.prepare("SELECT stamp, message FROM events ORDER BY stamp DESC LIMIT ?")?
        .query(&[limit])?
        .map(parse_event_row)
        .collect()
}

pub(crate) fn get_latest_event_like(
    conn: &PooledConnection<SqliteConnectionManager>,
    like: &str,
) -> Result<Option<Event>> {
    let mut events = conn.prepare("SELECT stamp, message FROM events WHERE message like ? ORDER BY stamp DESC LIMIT 1")?
        .query(params![like])?
        .map(parse_event_row)
        .collect::<Vec<Event>>()?;
    if events.is_empty() {
        Ok(None)
    } else {
        Ok(events.pop())
    }
}


pub(crate) fn insert_measurement(
    conn: &PooledConnection<SqliteConnectionManager>,
    stamp: DateTime<Utc>,
    measurement: &Measurement,
) -> Result<usize> {
    conn.execute(
        "INSERT INTO environment_measurements (stamp, temperature, humidity) VALUES (?1, ?2, ?3)",
        params![stamp, measurement.temperature, measurement.humidity,],
    )
}

pub(crate) fn get_latest_event(conn: &PooledConnection<SqliteConnectionManager>) -> Option<Event> {
    match get_latest_events(conn, 1) {
        Ok(mut events) => events.pop(),
        _ => None,
    }
}

pub(crate) fn get_latest_measurement(
    conn: &PooledConnection<SqliteConnectionManager>,
) -> Option<Event> {
    let result = conn.query_row(
        "SELECT stamp, temperature, humidity FROM environment_measurements ORDER BY stamp DESC LIMIT 1",
        NO_PARAMS,
        parse_measurement_row,
    );
    match result {
        Ok(event) => Some(event),
        _ => None,
    }
}

fn parse_event_row(row: &Row<'_>) -> Result<Event> {
    let message_str: String = row.get(1)?;
    match serde_json::from_str(&message_str) {
        Ok(message) => Ok(Event::raw(row.get(0)?, message)),
        Err(err) => Err(FromSqlError::Other(Box::new(err)).into()),
    }
}

fn parse_measurement_row(row: &Row<'_>) -> Result<Event> {
    Ok(Event::raw(
        row.get(0)?,
        Message::Environment(EnvironmentEvent::Measurement(Measurement::new(
            row.get(1)?,
            row.get(2)?,
        ))),
    ))
}

pub(crate) fn queue_event(
    conn: &PooledConnection<SqliteConnectionManager>,
    event: &Event,
) -> Result<usize> {
    insert_event_to(&"event_queue", conn, event)
}

pub(crate) fn dequeue_events(conn: &PooledConnection<SqliteConnectionManager>) -> Result<Vec<Event>> {
    let token: u32 = rand::thread_rng().gen_range(2, std::u32::MAX);
    conn.execute(
        "UPDATE event_queue SET group_token = ?1, stamp = ?2 WHERE group_token = 0",
        params![token, Utc::now()]
    )?;
    let events = conn.prepare("SELECT stamp, message FROM event_queue WHERE group_token = ?1 ORDER BY stamp")?
        .query(params![token])?
        .map(parse_event_row)
        .collect()?;
    conn.execute("UPDATE event_queue SET group_token = 1 WHERE group_token = ?1", params![token])?;
    Ok(events)
}

fn insert_event_to(
    table: &str,
    conn: &PooledConnection<SqliteConnectionManager>,
    event: &Event,
) -> Result<usize> {
    let query = format!("INSERT INTO {} (stamp, message) VALUES (?1, ?2)", table);
    conn.execute(
        query.as_str(),
        params![
            event.stamp(),
            serde_json::to_string(event.message()).unwrap()
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dequeue_events_removes_events() {
        {
            // arrange
            let pool = setup_db(&"./test.db");
            let conn = pool.get().unwrap();
            let event = Event::new(Message::Stop);

            // act
            queue_event(&conn, &event).unwrap();
            queue_event(&conn, &event).unwrap();

            let events1 = dequeue_events(&conn).unwrap();
            let events2 = dequeue_events(&conn).unwrap();

            // assert
            assert_eq!(events1.len(), 2);
            assert_eq!(events2.len(), 0);
        }
        std::fs::remove_file("./test.db").unwrap();
    }
}
