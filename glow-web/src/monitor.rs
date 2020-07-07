use std::time::Duration;

use actix::prelude::*;
use chrono::offset::Utc;
use log::error;

use crate::store::{Store, StorePool};

pub struct EventsMonitor<P: StorePool> {
    pool: P,
    count: u32,
}

impl<P: StorePool + 'static> EventsMonitor<P> {
    pub fn new(pool: P) -> EventsMonitor<P> {
        EventsMonitor { pool, count: 0 }
    }

    fn hb(&mut self, _ctx: &mut Context<Self>) {
        if is_alarming(&self.pool.get().unwrap(), self.count) {
            error!("device not emitting events");
        }
        self.count += 1;
    }
}

fn is_alarming(store: &impl Store, count: u32) -> bool {
    match store.get_latest_event() {
        // If we have an event check how recently it was received
        Some(event) => {
            let elapsed = Utc::now().signed_duration_since(event.stamp());
            elapsed > chrono::Duration::minutes(3)
        }
        // If we have no events check that we've been up for a little while
        None => count > 10,
    }
}

impl<P: StorePool + 'static> Actor for EventsMonitor<P> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is alive");

        ctx.run_interval(Duration::from_secs(3), move |act, ctx| {
            act.hb(ctx);
        });
    }
}
