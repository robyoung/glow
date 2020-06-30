use chrono::{offset::Utc, DateTime, Duration};
use fallible_iterator::FallibleIterator;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::{self, SqliteConnectionManager};
use rand::Rng;
use rusqlite::{types::FromSqlError, Result, Row, NO_PARAMS};

use glow_events::{
    v2::{Command, Event, Message, Payload},
    Measurement,
};

pub trait Store {
    fn migrate_db(&self);

    fn add_event(&self, message: &Message) -> Result<()>;

    fn get_latest_events(&self, limit: u32) -> Result<Vec<Message>>;

    fn get_latest_event(&self) -> Option<Message> {
        match self.get_latest_events(1) {
            Ok(mut events) => events.pop(),
            _ => None,
        }
    }

    fn get_latest_event_like(&self, like: &str) -> Result<Option<Message>>;

    fn add_measurement(&self, stamp: DateTime<Utc>, measurement: &Measurement) -> Result<()>;
    fn get_latest_measurement(&self) -> Option<Message>;
    fn get_measurements_since(&self, stamp: Duration) -> Result<Vec<Message>>;

    fn queue_command(&self, command: Command) -> Result<()>;
    fn dequeue_commands(&self) -> Result<Vec<Message>>;
}

pub trait StorePool: std::marker::Unpin {
    type Store: Store;

    fn get(&self) -> std::result::Result<Self::Store, r2d2::Error>;
}

pub struct SQLiteStore {
    conn: PooledConnection<SqliteConnectionManager>,
    now: fn() -> DateTime<Utc>,
}

impl SQLiteStore {
    fn new(
        conn: PooledConnection<SqliteConnectionManager>,
        now: fn() -> DateTime<Utc>,
    ) -> Self {
        Self { conn, now }
    }
}

impl Store for SQLiteStore {
    fn migrate_db(&self) {
        self.conn
            .execute(
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
        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS events_stamp ON events (stamp);",
                params![],
            )
            .expect("Cannot create events.stamp index");

        self.conn
            .execute(
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

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS environment_measurements_stamp ON environment_measurements (stamp);",
            params![],
        )
        .expect("Cannot create events.stamp index");

        self.conn
            .execute(
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

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS commands_created_at ON commands (stamp, group_token);",
                params![],
            )
            .expect("Cannot create commands.stamp index");

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS commands_group_token ON commands (group_token);",
                params![],
            )
            .expect("Cannot create commands.group_token index");
    }

    fn add_event(&self, message: &Message) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO events (stamp, payload) VALUES (?1, ?2)",
                params![
                    message.stamp(),
                    serde_json::to_string(message.payload()).unwrap()
                ],
            )
            .map(|_| ())
    }

    fn get_latest_events(&self, limit: u32) -> Result<Vec<Message>> {
        self.conn
            .prepare("SELECT stamp, payload FROM events ORDER BY stamp DESC LIMIT ?")?
            .query(&[limit])?
            .map(parse_message_row)
            .collect()
    }

    fn get_latest_event_like(&self, like: &str) -> Result<Option<Message>> {
        let mut events = self.conn
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

    fn add_measurement(&self, stamp: DateTime<Utc>, measurement: &Measurement) -> Result<()> {
        self.conn.execute(
            "INSERT INTO environment_measurements (stamp, temperature, humidity) VALUES (?1, ?2, ?3)",
            params![stamp, measurement.temperature, measurement.humidity,],
        ).map(|_| ())
    }

    fn get_latest_measurement(&self) -> Option<Message> {
        let result = self.conn.query_row(
            "SELECT stamp, temperature, humidity FROM environment_measurements ORDER BY stamp DESC LIMIT 1",
            NO_PARAMS,
            parse_measurement_row,
        );
        match result {
            Ok(event) => Some(event),
            _ => None,
        }
    }

    fn get_measurements_since(&self, since: Duration) -> Result<Vec<Message>> {
        let now = self.now;
        let now = now();
        let then = now.checked_sub_signed(since).unwrap();
        eprintln!("now: {:?}, then: {:?}", now, then);
        self.conn.prepare("SELECT stamp, temperature, humidity FROM environment_measurements WHERE stamp >= ? ORDER BY stamp DESC")?
            .query(params![then])?
            .map(parse_measurement_row)
            .collect::<Vec<Message>>()
    }

    fn queue_command(&self, command: Command) -> Result<()> {
        insert_message_to(&"commands", &self.conn, &Message::command(command)).map(|_| ())
    }

    fn dequeue_commands(&self) -> Result<Vec<Message>> {
        let token: u32 = rand::thread_rng().gen_range(2, std::u32::MAX);
        self.conn.execute(
            "UPDATE commands SET group_token = ?1, stamp = ?2 WHERE group_token = 0",
            params![token, Utc::now()],
        )?;
        let commands = self
            .conn
            .prepare("SELECT stamp, payload FROM commands WHERE group_token = ?1 ORDER BY stamp")?
            .query(params![token])?
            .map(parse_message_row)
            .collect()?;
        self.conn.execute(
            "UPDATE commands SET group_token = 1 WHERE group_token = ?1",
            params![token],
        )?;
        Ok(commands)
    }
}

#[derive(Clone)]
pub struct SQLiteStorePool {
    pool: Pool<SqliteConnectionManager>,
    now: fn() -> DateTime<Utc>,
}

impl SQLiteStorePool {
    pub fn new(pool: Pool<SqliteConnectionManager>) -> Self {
        Self {
            pool,
            now: Utc::now,
        }
    }

    pub fn from_path(path: &str) -> Self {
        Self::new(Pool::new(SqliteConnectionManager::file(path)).unwrap())
    }
}

#[cfg(test)]
impl SQLiteStorePool {
    pub fn with_now(
        pool: Pool<SqliteConnectionManager>,
        now: fn() -> DateTime<Utc>,
    ) -> Self {
        Self { pool, now }
    }

    pub fn from_path_with_now(path: &str, now: fn() -> DateTime<Utc>) -> Self {
        Self::with_now(Pool::new(SqliteConnectionManager::file(path)).unwrap(), now)
    }
}

impl StorePool for SQLiteStorePool {
    type Store = SQLiteStore;

    fn get(&self) -> std::result::Result<Self::Store, r2d2::Error> {
        Ok(SQLiteStore::new(self.pool.get()?, self.now))
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

    struct TestDb(String);

    impl Drop for TestDb {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_file(&self.0) {
                eprintln!("Failed to drop TestDb {:?}", e);
            }
        }
    }

    fn then() -> DateTime<Utc> {
        "2012-12-12T12:00:00Z".parse::<DateTime<Utc>>().unwrap()
    }

    fn setup_test_db(name: &str) -> (impl StorePool, TestDb) {
        let pool = SQLiteStorePool::from_path_with_now(name, then);
        pool.get().unwrap().migrate_db();

        (pool, TestDb(name.to_owned()))
    }

    #[test]
    fn dequeue_events_removes_events() {
        // arrange
        let (pool, _resource) = setup_test_db(&"./test.db");
        let store = pool.get().unwrap();

        // act
        store.queue_command(Command::Stop).unwrap();
        store.queue_command(Command::Stop).unwrap();

        let commands1 = store.dequeue_commands().unwrap();
        let commands2 = store.dequeue_commands().unwrap();

        // assert
        assert_eq!(commands1.len(), 2);
        assert_eq!(commands2.len(), 0);
    }

    #[test]
    fn test_get_measurements_since() {
        // arrange
        let (pool, _resource) = setup_test_db(&"./test1.db");
        let store = pool.get().unwrap();

        vec![
            ("2012-12-12T11:00:00Z", 10.0),
            ("2012-12-12T11:10:00Z", 11.0),
            ("2012-12-12T11:20:00Z", 12.0),
            ("2012-12-12T11:30:00Z", 13.0),
            ("2012-12-12T11:40:00Z", 14.0),
            ("2012-12-12T11:50:00Z", 15.0),
        ]
        .iter()
        .for_each(|&(stamp, temp)| {
            store
                .add_measurement(
                    stamp.parse::<DateTime<Utc>>().unwrap(),
                    &Measurement::new(temp, 10.0),
                )
                .unwrap();
        });

        // act
        let measurements = store
            .get_measurements_since(Duration::minutes(30))
            .unwrap();

        // assert
        assert_eq!(measurements.len(), 3);
        assert_eq!(
            measurements,
            vec![
                ("2012-12-12T11:50:00Z", 15.0),
                ("2012-12-12T11:40:00Z", 14.0),
                ("2012-12-12T11:30:00Z", 13.0),
            ]
            .iter()
            .map(|(stamp, temp)| {
                Message::raw(
                    stamp.parse::<DateTime<Utc>>().unwrap(),
                    Payload::Event(Event::Measurement(Measurement::new(*temp, 10.0))),
                )
            })
            .collect::<Vec<_>>()
        );
    }
}
