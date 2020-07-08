use actix_web::FromRequest;
use chrono::{offset::Utc, DateTime, Duration};
use eyre::Result;
use fallible_iterator::FallibleIterator;
use futures::future::{err, ok, Ready};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::{self, SqliteConnectionManager};
use rand::Rng;
use rusqlite::{types::FromSqlError, Row, NO_PARAMS};

use crate::weather::{Forecast, Observation};
use glow_events::{
    v2::{Command, Event, Message, Payload},
    Measurement,
};

pub trait StorePool: std::marker::Unpin + Clone {
    type Store: Store;

    fn get(&self) -> Result<Self::Store>;
}

pub trait Store {
    fn migrate_db(&self);

    fn add_event(&self, message: &Message) -> Result<()>;

    fn get_latest_events(&self, limit: u32) -> Result<Vec<Message>>;

    // the point of this method is to swallow the error
    #[allow(clippy::match_wildcard_for_single_variants)]
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

    fn add_observation(&self, observation: &Observation) -> Result<()>;
    fn add_forecast(&self, forecast: &Forecast) -> Result<()>;
    fn get_observations_since(&self, stamp: Duration) -> Result<Vec<Observation>>;
}

#[derive(Clone)]
pub struct SQLiteStorePool {
    pool: Pool<SqliteConnectionManager>,
    now: fn() -> DateTime<Utc>,
}

impl SQLiteStorePool {
    fn new(pool: Pool<SqliteConnectionManager>) -> Self {
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
    pub(crate) fn memory() -> Self {
        Self::new(Pool::new(SqliteConnectionManager::memory()).unwrap())
    }

    pub(crate) fn with_now(
        pool: Pool<SqliteConnectionManager>,
        now: fn() -> DateTime<Utc>,
    ) -> Self {
        Self { pool, now }
    }

    pub(crate) fn memory_with_now(now: fn() -> DateTime<Utc>) -> Self {
        Self::with_now(Pool::new(SqliteConnectionManager::memory()).unwrap(), now)
    }
}

impl StorePool for SQLiteStorePool {
    type Store = SQLiteStore;

    fn get(&self) -> Result<Self::Store> {
        Ok(SQLiteStore::new(self.pool.get()?, self.now))
    }
}

pub struct SQLiteStore {
    conn: PooledConnection<SqliteConnectionManager>,
    now: fn() -> DateTime<Utc>,
}

impl SQLiteStore {
    fn new(conn: PooledConnection<SqliteConnectionManager>, now: fn() -> DateTime<Utc>) -> Self {
        Self { conn, now }
    }
}

impl FromRequest for SQLiteStore {
    type Config = ();
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        if let Some(store) = req
            .app_data::<actix_web::web::Data<SQLiteStorePool>>()
            .and_then(|pool: &actix_web::web::Data<SQLiteStorePool>| pool.get().ok())
        {
            ok(store)
        } else {
            err(actix_web::error::ErrorInternalServerError(
                "Could not retrieve SQLite store.",
            ))
        }
    }
}

// TODO: tear this up and throw it away, these tables are bonkers!
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

        self.conn
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS weather (
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    date_time DATETIME,
                    url TEXT,
                    type TEXT,
                    payload TEXT
                );
                "#,
                params![],
            )
            .expect("Cannot create weather table");
    }

    fn add_event(&self, message: &Message) -> Result<()> {
        Ok(self
            .conn
            .execute(
                "INSERT INTO events (stamp, payload) VALUES (?1, ?2)",
                params![
                    message.stamp(),
                    serde_json::to_string(message.payload()).unwrap()
                ],
            )
            .map(|_| ())?)
    }

    fn get_latest_events(&self, limit: u32) -> Result<Vec<Message>> {
        Ok(self
            .conn
            .prepare("SELECT stamp, payload FROM events ORDER BY stamp DESC LIMIT ?")?
            .query(&[limit])?
            .map(parse_message_row)
            .collect()?)
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
        Ok(self.conn.execute(
            "INSERT INTO environment_measurements (stamp, temperature, humidity) VALUES (?1, ?2, ?3)",
            params![stamp, measurement.temperature, measurement.humidity,],
        ).map(|_| ())?)
    }

    // the point of this method is to swallow the error
    #[allow(clippy::match_wildcard_for_single_variants)]
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
        Ok(self.conn.prepare("SELECT stamp, temperature, humidity FROM environment_measurements WHERE stamp >= ? ORDER BY stamp DESC")?
            .query(params![now().checked_sub_signed(since).unwrap()])?
            .map(parse_measurement_row)
            .collect::<Vec<Message>>()?)
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

    fn add_observation(&self, observation: &Observation) -> Result<()> {
        Ok(self
            .conn
            .execute(
                "INSERT INTO weather (date_time, url, type, payload) VALUES (?1, ?2, ?3, ?4)",
                params![
                    observation.date_time,
                    observation.url,
                    "observation",
                    serde_json::to_string(observation).unwrap()
                ],
            )
            .map(|_| ())?)
    }

    fn add_forecast(&self, forecast: &Forecast) -> Result<()> {
        Ok(self
            .conn
            .execute(
                "INSERT INTO weather (date_time, url, type, payload) VALUES (?1, ?2, ?3, ?4)",
                params![
                    forecast.date_time,
                    forecast.url,
                    "forecast",
                    serde_json::to_string(forecast).unwrap()
                ],
            )
            .map(|_| ())?)
    }

    fn get_observations_since(&self, since: Duration) -> Result<Vec<Observation>> {
        let now = self.now;
        Ok(self
            .conn
            .prepare(
                r#"
                SELECT payload
                FROM weather
                WHERE type='observation' AND date_time >= ? ORDER BY date_time DESC
            "#,
            )?
            .query(params![now().checked_sub_signed(since).unwrap()])?
            .map(parse_observation_row)
            .collect::<Vec<Observation>>()?)
    }
}

fn parse_observation_row(row: &Row<'_>) -> rusqlite::Result<Observation> {
    let data: String = row.get(0)?;
    serde_json::from_str(&data)
        .map_err(|err| -> rusqlite::Error { FromSqlError::Other(Box::new(err)).into() })
}

fn parse_message_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    let payload_str: String = row.get(1)?;
    match serde_json::from_str(&payload_str) {
        Ok(payload) => Ok(Message::raw(row.get(0)?, payload)),
        Err(err) => Err(FromSqlError::Other(Box::new(err)).into()),
    }
}

fn parse_measurement_row(row: &Row<'_>) -> rusqlite::Result<Message> {
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
    Ok(conn.execute(
        query.as_str(),
        params![
            message.stamp(),
            serde_json::to_string(message.payload()).unwrap()
        ],
    )?)
}

#[cfg(test)]
pub mod test {
    use chrono::{DateTime, Duration, Utc};
    use eyre::Result;
    use rand::prelude::*;

    use super::{SQLiteStorePool, Store, StorePool};
    use crate::weather::{Observation, WindDirection};
    use glow_events::Measurement;

    pub struct TestDb {
        pool: SQLiteStorePool,
    }

    impl TestDb {
        pub fn with_pool(pool: SQLiteStorePool) -> Self {
            let db = Self { pool };
            db.store().unwrap().migrate_db();
            db
        }

        pub fn with_now(now: fn() -> DateTime<Utc>) -> Self {
            Self::with_pool(SQLiteStorePool::memory_with_now(now))
        }

        pub fn pool(&self) -> &SQLiteStorePool {
            &self.pool
        }

        pub fn store(&self) -> Result<impl Store> {
            self.pool().get()
        }

        pub fn add_observations(
            store: &impl Store,
            num: u32,
            from: DateTime<Utc>,
            until: DateTime<Utc>,
        ) -> Result<()> {
            let mut rng = rand::thread_rng();

            let duration = (until - from).num_seconds();
            let step = duration / num as i64;

            for i in 0..num {
                store.add_observation(&Observation {
                    temperature: rng.gen_range(5, 25),
                    humidity: rng.gen_range(30, 70),
                    wind_speed: rng.gen_range(0, 15),
                    wind_direction: WindDirection::NorthNorthWesterly,
                    date_time: from + Duration::seconds(i as i64 * step),
                    point: (12.1, 12.2),
                    url: "https://example.org".to_string(),
                })?;
            }
            Ok(())
        }

        pub fn add_measurements(
            store: &impl Store,
            num: u32,
            from: DateTime<Utc>,
            until: DateTime<Utc>,
        ) -> Result<()> {
            let mut rng = rand::thread_rng();

            let duration = (until - from).num_seconds();
            let step = duration / num as i64;

            for i in 0..num {
                store.add_measurement(
                    from + Duration::seconds(i as i64 * step),
                    &Measurement::new(rng.gen_range(5.0, 25.0), rng.gen_range(30.0, 70.0)),
                )?;
            }

            Ok(())
        }
    }

    impl Default for TestDb {
        fn default() -> Self {
            TestDb::with_pool(SQLiteStorePool::memory())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn then() -> DateTime<Utc> {
        "2012-12-12T12:00:00Z".parse::<DateTime<Utc>>().unwrap()
    }

    #[test]
    fn dequeue_events_removes_events() {
        // arrange
        let db = test::TestDb::with_now(then);
        let store = db.store().unwrap();

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
        let db = test::TestDb::with_now(then);
        let store = db.store().unwrap();

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
        let measurements = store.get_measurements_since(Duration::minutes(30)).unwrap();

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

    #[test]
    fn get_observations_since() {
        // arrange
        let db = test::TestDb::default();
        let store = db.store().unwrap();
        test::TestDb::add_observations(&store, 100, Utc::now() - Duration::hours(4), Utc::now())
            .unwrap();

        // act
        let observations = store.get_observations_since(Duration::minutes(61)).unwrap();

        // assert
        assert_eq!(observations.len(), 25);
    }
}
