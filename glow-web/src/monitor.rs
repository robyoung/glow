use std::time::Duration;

use actix::prelude::*;
use chrono::offset::Utc;

use crate::store::{Store, StorePool};

pub struct EventsMonitor<P: StorePool> {
    pool: P
}

impl<P: StorePool + 'static> EventsMonitor<P> {
    pub fn new(pool: P) -> EventsMonitor<P> {
        EventsMonitor { pool }
    }

    fn hb(&self, ctx: &mut Context<Self>, count: u32) {
        let store = self.pool.get().unwrap();

        ctx.run_later(Duration::new(3, 0), move |act, ctx| {
            if is_alarming(&store, count) {
                // TODO: send alarm
                println!("device not emitting events");
            }

            act.hb(ctx, count + 1);
        });
    }
}

fn is_alarming(store: &impl Store, count: u32) -> bool {
    match store.get_latest_event() {
        Some(event) => {
            let elapsed = Utc::now().signed_duration_since(event.stamp());
            elapsed > chrono::Duration::minutes(3)
        }
        None => count > 10,
    }
}

impl<P: StorePool + 'static> Actor for EventsMonitor<P> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is alive");

        self.hb(ctx, 0);
    }
}
