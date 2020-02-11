use chrono::{offset::Utc, DateTime};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::{self, SqliteConnectionManager};
use rusqlite::{types::FromSqlError, Result, NO_PARAMS};

use glow_events::{Event, Measurement};

pub fn setup_db(db_path: String) -> Pool<SqliteConnectionManager> {
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
}

pub(crate) fn insert_event(
    conn: &PooledConnection<SqliteConnectionManager>,
    event: &Event,
) -> Result<usize> {
    conn.execute(
        r#"INSERT INTO events (stamp, message) VALUES (?1, ?2)"#,
        params![
            event.stamp(),
            serde_json::to_string(event.message()).unwrap(),
        ],
    )
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
    let result = conn.query_row(
        "SELECT stamp, message FROM events ORDER BY stamp DESC LIMIT 1",
        NO_PARAMS,
        |row| {
            let message_str: String = row.get(1)?;
            match serde_json::from_str(&message_str) {
                Ok(message) => Ok(Event::raw(row.get(0)?, message)),
                Err(err) => Err(FromSqlError::Other(Box::new(err)).into()),
            }
        },
    );
    match result {
        Ok(event) => Some(event),
        _ => None,
    }
}
