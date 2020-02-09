#[macro_use]
extern crate rusqlite;

use actix_web::{dev::ServiceRequest, web, Error, HttpResponse, Responder};
use actix_web_httpauth::{
    extractors::{bearer::BearerAuth, AuthenticationError},
    headers::www_authenticate::bearer::Bearer,
};
use r2d2::Pool;
use r2d2_sqlite::{self, SqliteConnectionManager};

use glow_events::{EnvironmentEvent, Event, Message};

pub mod store;

use crate::store::{insert_event, insert_measurement};

pub struct AppState {
    pub token: String,
    pub password: String,
}

pub async fn index(state: web::Data<AppState>) -> impl Responder {
    HttpResponse::Ok().body(format!("Hi: {} {}", state.token, state.password))
}

pub async fn store_events(
    pool: web::Data<Pool<SqliteConnectionManager>>,
    events: web::Json<Vec<Event>>,
) -> impl Responder {
    let conn = pool.get().unwrap();

    for event in events.0.iter() {
        insert_event(&conn, event).unwrap();
        if let Message::Environment(EnvironmentEvent::Measurement(measurement)) = event.message() {
            insert_measurement(&conn, event.stamp(), measurement).unwrap();
        }
    }
    let return_events: Vec<Event> = vec![];
    HttpResponse::Ok().json(return_events)
}

pub async fn bearer_validator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, Error> {
    if let Some(state) = req.app_data::<AppState>() {
        if state.token == credentials.token() {
            return Ok(req);
        }
    }
    Err(AuthenticationError::new(Bearer::default()).into())
}
