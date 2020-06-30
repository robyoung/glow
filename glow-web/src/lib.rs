#[deny(clippy::pedantic)]
#[macro_use]
extern crate rusqlite;

use std::task::{Context, Poll};

use actix_service::{Service, Transform};
use actix_session::UserSession;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    http, Error, HttpResponse,
};
use actix_web_httpauth::{
    extractors::{bearer::BearerAuth, AuthenticationError},
    headers::www_authenticate::bearer::Bearer,
};
use futures::future::{ok, Either, Ready};

mod formatting;
mod monitor;
pub mod routes;
pub mod store;

pub use crate::monitor::EventsMonitor;

pub struct AppState {
    pub token: String,
    pub password: String,
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

pub struct CheckLogin;

impl<S, B> Transform<S> for CheckLogin
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = CheckLoginMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(CheckLoginMiddleware { service })
    }
}
pub struct CheckLoginMiddleware<S> {
    service: S,
}

impl<S, B> Service for CheckLoginMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Either<S::Future, Ready<Result<Self::Response, Self::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let authenticated: bool = req
            .get_session()
            .get("authenticated")
            .unwrap_or(None)
            .unwrap_or(false);

        if authenticated {
            Either::Left(self.service.call(req))
        } else {
            // Don't forward to /login if we are already on /login
            if req.path() == "/login" {
                Either::Left(self.service.call(req))
            } else {
                Either::Right(ok(req.into_response(found("/login"))))
            }
        }
    }
}

pub(crate) fn found<B>(location: &str) -> HttpResponse<B> {
    HttpResponse::Found()
        .header(http::header::LOCATION, location)
        .finish()
        .into_body()
}
