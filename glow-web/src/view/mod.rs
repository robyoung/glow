use std::sync::Arc;

use eyre::Result;
use futures::future::{err, ok, Ready};
use serde::Serialize;

pub mod data;

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
        req.app_data::<actix_web::web::Data<tera::Tera>>().map_or(
            err(actix_web::error::ErrorInternalServerError(
                "Could not build template view",
            )),
            |tmpl| ok(TeraView::new(tmpl.clone().into_inner())),
        )
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
pub mod test {
    use std::collections::HashMap;

    use eyre::Result;
    use serde::{de::DeserializeOwned, Serialize};

    use super::View;

    #[derive(Default)]
    pub struct TestView {
        ctx: HashMap<String, String>,
    }

    impl TestView {
        pub fn get<T: DeserializeOwned + std::marker::Sized, S: Into<String>>(
            &self,
            key: S,
        ) -> Option<T> {
            let value = dbg!(self.ctx.get(&key.into()));

            value.map(|val| serde_json::from_str(val).unwrap())
        }
    }

    impl View for TestView {
        fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T) {
            self.ctx
                .insert(key.into(), serde_json::to_string(val).unwrap());
        }

        fn render(&self, template: &str) -> Result<String> {
            Ok(template.to_string())
        }
    }
}
