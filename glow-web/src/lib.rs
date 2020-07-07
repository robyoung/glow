#[deny(clippy::pedantic)]
#[macro_use]
extern crate rusqlite;

mod authentication;
mod controllers;
mod data;
mod formatting;
mod monitor;
pub mod routes;
mod session;
mod store;
mod view;
mod weather;

pub use crate::authentication::{bearer_validator, CheckLogin};
pub use crate::monitor::EventsMonitor;
pub use crate::store::SQLiteStorePool;
pub use crate::weather::{BBCWeatherService, WeatherMonitor};

pub struct AppState {
    pub token: String,
    pub password: String,
}
