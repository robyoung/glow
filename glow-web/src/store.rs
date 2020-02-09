use chrono::{offset::Utc, DateTime};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::{self, SqliteConnectionManager};
use rusqlite::Result;

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
}

pub(crate) fn insert_event(
    conn: &PooledConnection<SqliteConnectionManager>,
    event: &Event,
) -> Result<usize> {
    conn.execute(
        r#"INSERT INTO events (stamp, message) VALUES (?1, ?2)"#,
        params![
            event.stamp().timestamp(),
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
        params![
            stamp.timestamp(),
            measurement.temperature,
            measurement.humidity,
        ],
    )
}
