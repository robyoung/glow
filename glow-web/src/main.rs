extern crate glow_web;

use actix::Actor;
use actix_session::CookieSession;
use actix_web::{middleware::Logger, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use base64;
use env_logger;
use tera::Tera;

use glow_web::{bearer_validator, routes, store, AppState, CheckLogin, EventsMonitor};

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let db_path = std::env::var("DB_PATH").expect("DB_PATH is required");
    let app_token = std::env::var("APP_TOKEN").expect("APP_TOKEN is required");
    let app_password =
        base64::decode(&std::env::var("APP_PASSWORD").expect("APP_PASSWORD is required"))
            .expect("APP_PASSWORD is not valid base64");
    let cookie_key =
        base64::decode(&std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET is required"))
            .expect("COOKIE_SECRET is not valid base64");

    let pool = store::setup_db(db_path);

    EventsMonitor::new(pool.clone()).start();

    HttpServer::new(move || {
        let app_token = app_token.clone();
        let app_password = app_password.clone();
        let cookie_key = cookie_key.clone();
        let tera = Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*")).unwrap();

        App::new()
            .wrap(Logger::default())
            .wrap(
                CookieSession::signed(&cookie_key)
                    .name("glow")
                    .http_only(true)
                    .secure(false),
            )
            .data(AppState {
                token: app_token,
                password: std::str::from_utf8(&app_password).unwrap().to_string(),
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
            .service(
                web::resource("/login")
                    .route(web::get().to(routes::login))
                    .route(web::post().to(routes::do_login)),
            )
            .service(
                web::scope("/")
                    .wrap(CheckLogin)
                    .route("", web::get().to(routes::index))
                    .route("/logout", web::get().to(routes::logout)),
            )
    })
    .bind("0.0.0.0:8088")?
    .run()
    .await
}
