use std::time::Duration;

use actix::prelude::*;
use chrono::offset::Utc;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;

use crate::store::get_latest_event;

pub struct EventsMonitor {
    pool: Pool<SqliteConnectionManager>,
}

impl EventsMonitor {
    pub fn new(pool: Pool<SqliteConnectionManager>) -> EventsMonitor {
        EventsMonitor { pool }
    }

    fn hb(&self, ctx: &mut Context<Self>, count: u32) {
        let pool = self.pool.clone();
        ctx.run_later(Duration::new(3, 0), move |act, ctx| {
            let conn = pool.get().unwrap();

            if is_alarming(&conn, count) {
                // TODO: send alarm
                println!("device not emitting events");
            }

            act.hb(ctx, count + 1);
        });
    }
}

fn is_alarming(conn: &PooledConnection<SqliteConnectionManager>, count: u32) -> bool {
    match get_latest_event(conn) {
        Some(event) => {
            let elapsed = Utc::now().signed_duration_since(event.stamp());
            println!("elapsed: {:?}", elapsed);
            elapsed > chrono::Duration::seconds(30)
        }
        None => count > 10,
    }
}

impl Actor for EventsMonitor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is alive");

        self.hb(ctx, 0);
    }
}
