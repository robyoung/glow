use actix_web::{error, http, web, Error, HttpResponse, Responder};
use serde_json::json;

use glow_events::v2::Message;

use crate::{
    controllers,
    data::{Login, SetBrightness},
    session::ActixSession,
    store,
    view::{TeraView, View},
    AppData,
};

pub async fn status() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

pub async fn index(
    store: store::SQLiteStore,
    mut view: TeraView,
    mut session: ActixSession,
) -> Result<HttpResponse, Error> {
    ok_html(controllers::index(&store, &mut view, &mut session))
}

pub async fn set_brightness(
    form: web::Form<SetBrightness>,
    store: store::SQLiteStore,
    mut session: ActixSession,
) -> Result<HttpResponse, Error> {
    map_err(controllers::set_brightness(
        &store,
        &mut session,
        form.brightness as f32 / 100.0,
    ))?;

    Ok(found("/"))
}

pub async fn list_devices(
    store: store::SQLiteStore,
    mut session: ActixSession,
) -> Result<HttpResponse, Error> {
    map_err(controllers::list_devices(&store, &mut session))?;

    Ok(found("/"))
}

pub async fn run_heater(
    store: store::SQLiteStore,
    mut session: ActixSession,
) -> Result<HttpResponse, Error> {
    map_err(controllers::run_heater(&store, &mut session))?;

    Ok(found("/"))
}

pub async fn stop_device(
    store: store::SQLiteStore,
    mut session: ActixSession,
) -> Result<HttpResponse, Error> {
    map_err(controllers::stop_device(&store, &mut session))?;

    Ok(found("/"))
}

pub async fn login(view: TeraView) -> impl Responder {
    ok_html(view.render("login.html"))
}

pub async fn do_login(
    form: web::Form<Login>,
    state: web::Data<AppData>,
    session: ActixSession,
) -> Result<HttpResponse, Error> {
    if map_err(controllers::sign_in(
        &session,
        &state.password,
        &form.password,
    ))? {
        Ok(found("/"))
    } else {
        Err(error::ErrorUnauthorized("bad password"))
    }
}

pub async fn logout(session: ActixSession) -> Result<HttpResponse, Error> {
    map_err(controllers::sign_out(&session))?;
    Ok(found("/login"))
}

pub async fn store_events(
    store: store::SQLiteStore,
    events: web::Json<Vec<Message>>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(map_err(controllers::store_events(&store, events.0))?))
}

pub async fn list_events(store: store::SQLiteStore) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(map_err(controllers::list_events(&store))?))
}

pub(crate) fn found<B>(location: &str) -> HttpResponse<B> {
    HttpResponse::Found()
        .header(http::header::LOCATION, location)
        .finish()
        .into_body()
}

/// Wrap a rendered html body in an actix response
fn ok_html(body: eyre::Result<String>) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok()
        .content_type("text/html")
        .body(map_err(body)?))
}

fn map_err<T>(r: eyre::Result<T>) -> Result<T, Error> {
    r.map_err(|e| error::ErrorInternalServerError(e))
}
