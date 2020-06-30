use actix_session::Session;
use actix_web::{error, web, Error, HttpResponse, Responder};
use argon2;
use chrono::offset::Utc;
use serde::Deserialize;
use serde_json::json;

use glow_events::v2::{Command, Event, Message, Payload};

use crate::controllers::{self, ActixSession};
use crate::store::{Store, StorePool};
use crate::view::TeraView;
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

fn ok_html(body: eyre::Result<String>) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok()
        .content_type("text/html")
        .body(body.map_err(|e| error::ErrorInternalServerError(e))?))
}

pub async fn status() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

pub async fn index(
    pool: web::Data<store::SQLiteStorePool>,
    tmpl: web::Data<tera::Tera>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let store = pool.get().map_err(WrappedError::from)?;
    let mut view = TeraView::new(&tmpl);
    let mut session = ActixSession::new(session);

    ok_html(controllers::index(&store, &mut view, &mut session))
}

#[derive(Deserialize)]
pub struct SetBrightnessForm {
    brightness: u32,
}

pub async fn set_brightness(
    form: web::Form<SetBrightnessForm>,
    pool: web::Data<store::SQLiteStorePool>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let store = pool.get().map_err(WrappedError::from)?;
    store
        .queue_command(Command::SetBrightness(form.brightness as f32 / 100.0))
        .map_err(|_| error::ErrorInternalServerError("failed to queue brightness event"))?;
    session.set("flash", "set brightness event queued")?;
    Ok(found("/"))
}

pub async fn list_devices(
    pool: web::Data<store::SQLiteStorePool>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let store = pool.get().map_err(WrappedError::from)?;
    store
        .queue_command(Command::ListDevices)
        .map_err(|_| error::ErrorInternalServerError("failed to request device list"))?;

    session.set("flash", "list devices request sent")?;

    Ok(found("/"))
}

pub async fn run_heater(
    pool: web::Data<store::SQLiteStorePool>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let store = pool.get().map_err(WrappedError::from)?;
    let latest_event = store
        .get_latest_event_like(&r#"{"TPLink":"RunHeater"}"#)
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
        store
            .queue_command(Command::RunHeater)
            .map_err(|_| error::ErrorInternalServerError("failed to run heater event"))?;
        session.set("flash", "run heater event queued")?;
    } else {
        session.set("flash", "cannot queue run heater event")?;
    }

    Ok(found("/"))
}

pub async fn stop_device(
    pool: web::Data<store::SQLiteStorePool>,
    session: Session,
) -> Result<HttpResponse, Error> {
    let store = pool.get().map_err(WrappedError::from)?;
    store
        .queue_command(Command::Stop)
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
    pool: web::Data<store::SQLiteStorePool>,
    events: web::Json<Vec<Message>>,
) -> impl Responder {
    let store = pool.get().map_err(WrappedError::from)?;
    for event in events.0.iter() {
        store.add_event(event).unwrap();
        if let Payload::Event(Event::Measurement(measurement)) = event.payload() {
            store.add_measurement(event.stamp(), measurement).unwrap();
        }
    }
    store
        .dequeue_commands()
        .map(|commands| HttpResponse::Ok().json(commands))
        .map_err(|err| error::ErrorInternalServerError(format!("{}", err)))
}

pub async fn list_events(pool: web::Data<store::SQLiteStorePool>) -> Result<HttpResponse, Error> {
    let store = pool.get().map_err(WrappedError::from)?;

    Ok(HttpResponse::Ok().json(store.get_latest_events(20).unwrap()))
}

#[derive(Debug)]
struct WrappedError(Error);

impl From<eyre::Error> for WrappedError {
    fn from(e: eyre::Error) -> Self {
        Self(error::ErrorInternalServerError(e))
    }
}

impl From<WrappedError> for Error {
    fn from(e: WrappedError) -> Error {
        e.0
    }
}
