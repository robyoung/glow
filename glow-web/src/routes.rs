use actix_session::Session;
use actix_web::{error, web, Error, HttpResponse, Responder};
use argon2;
use chrono::offset::Utc;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use serde_json::json;

use glow_events::{EnvironmentEvent, Event, Message};

use crate::{found, store, AppState};
use crate::formatting::{format_time_since, EventSummary};

fn render(
    tmpl: web::Data<tera::Tera>,
    template_name: &str,
    context: Option<&tera::Context>,
) -> Result<HttpResponse, Error> {
    let body = tmpl
        .render(template_name, context.unwrap_or(&tera::Context::new()))
        .map_err(|_| error::ErrorInternalServerError("template errror"))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

pub async fn status() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

pub async fn index(
    pool: web::Data<Pool<SqliteConnectionManager>>,
    tmpl: web::Data<tera::Tera>,
) -> Result<HttpResponse, Error> {
    let conn = pool.get().unwrap();
    let mut ctx = tera::Context::new();
    if let Some(event) = store::get_latest_measurement(&conn) {
        ctx.insert("event", &event);
        if let Message::Environment(EnvironmentEvent::Measurement(measurement)) = event.message() {
            ctx.insert("measurement", measurement);
            ctx.insert(
                "measurement_age",
                &format_time_since(Utc::now(), event.stamp()),
            );
            let events = match store::get_latest_events(&conn, 20) {
                Ok(events) => events,
                Err(_) => Vec::new(),
            };
            ctx.insert("events", &events.iter().map(EventSummary::from).collect::<Vec<EventSummary>>());
        }
    }
    render(tmpl, "index.html", Some(&ctx))
}

pub async fn login(tmpl: web::Data<tera::Tera>) -> impl Responder {
    render(tmpl, "login.html", None)
}

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

pub async fn do_login(
    form: web::Form<LoginForm>,
    state: web::Data<AppState>,
    session: Session,
) -> Result<HttpResponse, Error> {
    if argon2::verify_encoded(&state.password, form.password.as_bytes()).unwrap() {
        session.set("authenticated", true)?;
        Ok(found("/"))
    } else {
        Err(error::ErrorUnauthorized("bad password"))
    }
}

pub async fn logout(session: Session) -> Result<HttpResponse, Error> {
    session.set("authenticated", false)?;
    Ok(found("/login"))
}

pub async fn store_events(
    pool: web::Data<Pool<SqliteConnectionManager>>,
    events: web::Json<Vec<Event>>,
) -> impl Responder {
    let conn = pool.get().unwrap();

    for event in events.0.iter() {
        store::insert_event(&conn, event).unwrap();
        if let Message::Environment(EnvironmentEvent::Measurement(measurement)) = event.message() {
            store::insert_measurement(&conn, event.stamp(), measurement).unwrap();
        }
    }
    let return_events: Vec<Event> = vec![];
    HttpResponse::Ok().json(return_events)
}

pub async fn list_events(pool: web::Data<Pool<SqliteConnectionManager>>) -> impl Responder {
    let conn = pool.get().unwrap();

    HttpResponse::Ok().json(store::get_latest_events(&conn, 20).unwrap())
}
