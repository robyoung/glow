use actix_session::Session;
use actix_web::{error, web, Error, HttpResponse, Responder};
use argon2;
use chrono::offset::Utc;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use serde_json::json;

use glow_events::v2::{Command, Event, Message, Payload};

use crate::formatting::{format_time_since, EventSummary};
use crate::{found, store, AppState};

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
    session: Session,
) -> Result<HttpResponse, Error> {
    let conn = pool.get().unwrap();
    let mut ctx = tera::Context::new();

    let flash: Option<String> = session
        .get("flash")
        .map_err(|_| error::ErrorInternalServerError("invalid flash message"))?;
    if flash.is_some() {
        session.remove("flash");
    }
    ctx.insert("flash", &flash);

    if let Some(message) = store::get_latest_measurement(&conn) {
        if let Payload::Event(Event::Measurement(measurement)) = message.payload() {
            ctx.insert("measurement", measurement);
            ctx.insert(
                "measurement_age",
                &format_time_since(Utc::now(), message.stamp()),
            );
        }
    }
    let events = match store::get_latest_events(&conn, 20) {
        Ok(events) => events,
        Err(_) => Vec::new(),
    };
    let events = events
        .iter()
        .map(EventSummary::from)
        .collect::<Vec<EventSummary>>();
    ctx.insert("events", &events);

    render(tmpl, "index.html", Some(&ctx))
}

#[derive(Deserialize)]
pub struct SetBrightnessForm {
    brightness: u32,
}

pub async fn set_brightness(
    form: web::Form<SetBrightnessForm>,
    pool: web::Data<Pool<SqliteConnectionManager>>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let conn = pool
        .get()
        .map_err(|_| error::ErrorInternalServerError("cannot get db connection"))?;
    store::queue_command(
        &conn,
        Command::SetBrightness(form.brightness as f32 / 100.0),
    )
    .map_err(|_| error::ErrorInternalServerError("failed to queue brightness event"))?;
    session.set("flash", "set brightness event queued")?;
    Ok(found("/"))
}

pub async fn list_devices(
    pool: web::Data<Pool<SqliteConnectionManager>>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let conn = pool
        .get()
        .map_err(|_| error::ErrorInternalServerError("cannot get db connection"))?;

    store::queue_command(&conn, Command::ListDevices)
        .map_err(|_| error::ErrorInternalServerError("failed to request device list"))?;

    session.set("flash", "list devices request sent")?;

    Ok(found("/"))
}

pub async fn run_heater(
    pool: web::Data<Pool<SqliteConnectionManager>>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let conn = pool
        .get()
        .map_err(|_| error::ErrorInternalServerError("cannot get db connection"))?;

    let latest_event = store::get_latest_event_like(&conn, &r#"{"TPLink":"RunHeater"}"#)
        .map_err(|_| error::ErrorInternalServerError("failed to get latest heater event"))?;
    let can_run_heater = if let Some(latest_event) = latest_event {
        Utc::now()
            .signed_duration_since(latest_event.stamp())
            .num_minutes()
            > 2
    } else {
        true
    };

    if can_run_heater {
        store::queue_command(&conn, Command::RunHeater)
            .map_err(|_| error::ErrorInternalServerError("failed to run heater event"))?;
        session.set("flash", "run heater event queued")?;
    } else {
        session.set("flash", "cannot queue run heater event")?;
    }

    Ok(found("/"))
}

pub async fn stop_device(
    pool: web::Data<Pool<SqliteConnectionManager>>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let conn = pool
        .get()
        .map_err(|_| error::ErrorInternalServerError("cannot get db connection"))?;
    store::queue_command(&conn, Command::Stop)
        .map_err(|_| error::ErrorInternalServerError("failed to stop device"))?;
    session.set("flash", "stop event queued")?;

    Ok(found("/"))
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
    events: web::Json<Vec<Message>>,
) -> impl Responder {
    let conn = pool.get().unwrap();

    for event in events.0.iter() {
        store::insert_event(&conn, event).unwrap();
        if let Payload::Event(Event::Measurement(measurement)) = event.payload() {
            store::insert_measurement(&conn, event.stamp(), measurement).unwrap();
        }
    }
    store::dequeue_commands(&conn)
        .map(|commands| HttpResponse::Ok().json(commands))
        .map_err(|err| error::ErrorInternalServerError(format!("{}", err)))
}

pub async fn list_events(pool: web::Data<Pool<SqliteConnectionManager>>) -> impl Responder {
    let conn = pool.get().unwrap();

    HttpResponse::Ok().json(store::get_latest_events(&conn, 20).unwrap())
}
