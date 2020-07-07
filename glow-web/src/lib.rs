#[deny(clippy::pedantic)]
#[macro_use]
extern crate rusqlite;

use actix::Actor;
use actix_web::{middleware::Logger, App, HttpServer, web};
use tera::{Tera, Result as TeraResult};
use actix_session::CookieSession;
use actix_web_httpauth::middleware::HttpAuthentication;

use crate::authentication::{bearer_validator, CheckLogin};
use crate::data::AppData;
use crate::monitor::EventsMonitor;
use crate::store::SQLiteStorePool;
#[cfg(feature = "weather-monitor")]
use crate::weather::{BBCWeatherService, WeatherMonitor};

mod authentication;
mod controllers;
mod data;
mod formatting;
mod monitor;
mod routes;
mod session;
mod store;
mod view;
#[cfg(feature = "weather-monitor")]
mod weather;


/// Run the Glow web server
pub async fn run_server() -> std::io::Result<()> {
    let env = EnvironmentData::load();
    let tera = templates().expect("Could not load templates");
    let pool = SQLiteStorePool::from_path(&env.db_path);

    EventsMonitor::new(pool.clone()).start();
    #[cfg(feature = "weather-monitor")]
    WeatherMonitor::new(pool.clone(), BBCWeatherService::new(&env.weather_location)).start();

    HttpServer::new(move || {
        let env = env.clone();
        let tera = tera.clone();

        App::new()
            .wrap(Logger::default())
            .wrap(
                CookieSession::signed(&env.cookie_key)
                    .name("glow")
                    .http_only(true)
                    .secure(false)
                    .max_age(60 * 60 * 24 * 3),
            )
            .data(AppData {
                token: env.app_token,
                password: std::str::from_utf8(&env.app_password).unwrap().to_string(),
            })
            .data(pool.clone())
            .data(tera)
            .service(
                web::scope("/api")
                    .wrap(HttpAuthentication::bearer(bearer_validator))
                    .service(
                        web::resource("/events")
                            .route(web::post().to(routes::store_events))
                            .route(web::get().to(routes::list_events)),
                    ),
            )
            .service(web::resource("/status").route(web::get().to(routes::status)))
            .service(
                web::resource("/login")
                    .route(web::get().to(routes::login))
                    .route(web::post().to(routes::do_login)),
            )
            .service(
                web::scope("/")
                    .wrap(CheckLogin)
                    .route("", web::get().to(routes::index))
                    .route("/logout", web::get().to(routes::logout))
                    .route("/brightness", web::post().to(routes::set_brightness))
                    .route("/list-devices", web::post().to(routes::list_devices))
                    .route("/stop-device", web::post().to(routes::stop_device))
                    .route("/run-heater", web::post().to(routes::run_heater)),
            )
    })
    .bind("127.0.0.1:8088")?
    .run()
    .await
}

#[cfg(feature = "embedded-templates")]
fn templates() -> TeraResult<Tera> {
    let templates = vec![
        ("login.html", include_str!("../templates/login.html")),
        ("index.html", include_str!("../templates/index.html")),
        ("base.html", include_str!("../templates/base.html")),
    ];
    match Tera::new("/dev/null/*") {
        Ok(mut tera) => {
            tera.add_raw_templates(templates)?;
            Ok(tera)
        }
        Err(err) => Err(err),
    }
}

#[cfg(not(feature = "embedded-templates"))]
fn templates() -> TeraResult<Tera> {
    Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*"))
}

#[derive(Clone)]
struct EnvironmentData {
    db_path: String,
    app_token: String,
    app_password: Vec<u8>,
    cookie_key: Vec<u8>,
    weather_location: String,
}

impl EnvironmentData {
    pub fn load() -> Self {
        Self {
            db_path: std::env::var("DB_PATH").expect("DB_PATH is required"),
            app_token: std::env::var("APP_TOKEN").expect("APP_TOKEN is required"),
            app_password: base64::decode(
                &std::env::var("APP_PASSWORD").expect("APP_PASSWORD is required"),
            )
            .expect("APP_PASSWORD is not valid base64"),
            cookie_key: base64::decode(
                &std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET is required"),
            )
            .expect("COOKIE_SECRET is not valid base64"),
            weather_location: std::env::var("BBC_WEATHER_LOCATION")
                .expect("BBC_WEATHER_LOCATION is required"),
        }
    }
}
