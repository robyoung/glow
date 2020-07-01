#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;

use eyre::Result;
use futures::future::{err, ok, Ready};
use serde::Serialize;
#[cfg(test)]
use serde_json::value::{to_value, Value};

pub(crate) trait View {
    fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T);
    fn render(&self, template: &str) -> Result<String>;
}

pub struct TeraView {
    tera: Arc<tera::Tera>,
    ctx: tera::Context,
}

impl TeraView {
    pub(crate) fn new(tera: Arc<tera::Tera>) -> Self {
        Self {
            tera,
            ctx: tera::Context::new(),
        }
    }
}

impl actix_web::FromRequest for TeraView {
    type Config = ();
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        if let Some(tmpl) = req.app_data::<actix_web::web::Data<tera::Tera>>() {
            ok(TeraView::new(tmpl.clone().into_inner()))
        } else {
            err(actix_web::error::ErrorInternalServerError(
                "Could not build template view",
            ))
        }
    }
}

impl View for TeraView {
    fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T) {
        self.ctx.insert(key, val)
    }

    fn render(&self, template: &str) -> Result<String> {
        Ok(self.tera.render(template, &self.ctx)?)
    }
}

#[cfg(test)]
struct TestView {
    ctx: HashMap<String, Value>,
}

#[cfg(test)]
impl View for TestView {
    fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T) {
        self.ctx.insert(key.into(), to_value(val).unwrap());
    }

    fn render(&self, template: &str) -> Result<String> {
        Ok(template.to_string())
    }
}
