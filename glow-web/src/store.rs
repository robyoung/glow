use chrono::{offset::Utc, DateTime};
use fallible_iterator::FallibleIterator;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::{self, SqliteConnectionManager};
use rand::Rng;
use rusqlite::{types::FromSqlError, Result, Row, NO_PARAMS};

use glow_events::{
    v2::{Command, Event, Message, Payload},
    Measurement,
};

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
            payload TEXT
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
        CREATE TABLE IF NOT EXISTS commands (
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            stamp DATETIME,
            payload TEXT,
            group_token INT DEFAULT 0
        );
        "#,
        params![],
    )
    .expect("Cannot create commands table");
    conn.execute(
        "CREATE INDEX IF NOT EXISTS commands_created_at ON commands (stamp, group_token);",
        params![],
    )
    .expect("Cannot create commands.stamp index");
    conn.execute(
        "CREATE INDEX IF NOT EXISTS commands_group_token ON commands (group_token);",
        params![],
    )
    .expect("Cannot create commands.group_token index");
}

pub(crate) fn insert_event(
    conn: &PooledConnection<SqliteConnectionManager>,
    message: &Message,
) -> Result<usize> {
    conn.execute(
        "INSERT INTO events (stamp, payload) VALUES (?1, ?2)",
        params![
            message.stamp(),
            serde_json::to_string(message.payload()).unwrap()
        ],
    )
}

pub(crate) fn get_latest_events(
    conn: &PooledConnection<SqliteConnectionManager>,
    limit: u32,
) -> Result<Vec<Message>> {
    conn.prepare("SELECT stamp, payload FROM events ORDER BY stamp DESC LIMIT ?")?
        .query(&[limit])?
        .map(parse_message_row)
        .collect()
}

pub(crate) fn get_latest_event_like(
    conn: &PooledConnection<SqliteConnectionManager>,
    like: &str,
) -> Result<Option<Message>> {
    let mut events = conn
        .prepare(
            "SELECT stamp, payload FROM events WHERE payload like ? ORDER BY stamp DESC LIMIT 1",
        )?
        .query(params![like])?
        .map(parse_message_row)
        .collect::<Vec<Message>>()?;
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

pub(crate) fn get_latest_event(
    conn: &PooledConnection<SqliteConnectionManager>,
) -> Option<Message> {
    match get_latest_events(conn, 1) {
        Ok(mut events) => events.pop(),
        _ => None,
    }
}

pub(crate) fn get_latest_measurement(
    conn: &PooledConnection<SqliteConnectionManager>,
) -> Option<Message> {
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

fn parse_message_row(row: &Row<'_>) -> Result<Message> {
    let payload_str: String = row.get(1)?;
    match serde_json::from_str(&payload_str) {
        Ok(payload) => Ok(Message::raw(row.get(0)?, payload)),
        Err(err) => Err(FromSqlError::Other(Box::new(err)).into()),
    }
}

fn parse_measurement_row(row: &Row<'_>) -> Result<Message> {
    Ok(Message::raw(
        row.get(0)?,
        Payload::Event(Event::Measurement(Measurement::new(
            row.get(1)?,
            row.get(2)?,
        ))),
    ))
}

pub(crate) fn queue_command(
    conn: &PooledConnection<SqliteConnectionManager>,
    command: Command,
) -> Result<usize> {
    insert_message_to(&"commands", conn, &Message::command(command))
}

pub(crate) fn dequeue_commands(
    conn: &PooledConnection<SqliteConnectionManager>,
) -> Result<Vec<Message>> {
    let token: u32 = rand::thread_rng().gen_range(2, std::u32::MAX);
    conn.execute(
        "UPDATE commands SET group_token = ?1, stamp = ?2 WHERE group_token = 0",
        params![token, Utc::now()],
    )?;
    let commands = conn
        .prepare("SELECT stamp, payload FROM commands WHERE group_token = ?1 ORDER BY stamp")?
        .query(params![token])?
        .map(parse_message_row)
        .collect()?;
    conn.execute(
        "UPDATE commands SET group_token = 1 WHERE group_token = ?1",
        params![token],
    )?;
    Ok(commands)
}

fn insert_message_to(
    table: &str,
    conn: &PooledConnection<SqliteConnectionManager>,
    message: &Message,
) -> Result<usize> {
    let query = format!("INSERT INTO {} (stamp, payload) VALUES (?1, ?2)", table);
    conn.execute(
        query.as_str(),
        params![
            message.stamp(),
            serde_json::to_string(message.payload()).unwrap()
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

            // act
            queue_command(&conn, Command::Stop).unwrap();
            queue_command(&conn, Command::Stop).unwrap();

            let commands1 = dequeue_commands(&conn).unwrap();
            let commands2 = dequeue_commands(&conn).unwrap();

            // assert
            assert_eq!(commands1.len(), 2);
            assert_eq!(commands2.len(), 0);
        }
        std::fs::remove_file("./test.db").unwrap();
    }
}
