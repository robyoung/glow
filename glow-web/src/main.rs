extern crate glow_web;

use actix::Actor;
use actix_web::{middleware::Logger, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use env_logger;

use glow_web::{
    bearer_validator, index, list_events, store, store_events, AppState, EventsMonitor,
};

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let db_path = std::env::var("DB_PATH").expect("DB_PATH is required");
    let app_token = std::env::var("APP_TOKEN").expect("APP_TOKEN is required");
    let app_password = std::env::var("APP_PASSWORD").expect("APP_PASSWORD is required");

    let pool = store::setup_db(db_path);

    EventsMonitor::new(pool.clone()).start();

    HttpServer::new(move || {
        let app_token = app_token.clone();
        let app_password = app_password.clone();

        App::new()
            .wrap(Logger::default())
            .data(AppState {
                token: app_token,
                password: app_password,
            })
            .data(pool.clone())
            .route("/", web::get().to(index))
            .service(
                web::scope("/api")
                    .wrap(HttpAuthentication::bearer(bearer_validator))
                    .service(
                        web::resource("/events")
                            .route(web::post().to(store_events))
                            .route(web::get().to(list_events)),
                    ),
            )
    })
    .bind("127.0.0.1:8088")?
    .run()
    .await
}
