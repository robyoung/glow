use eyre::{eyre, Result};
use futures::future::{ok, Ready};
use serde::{de::DeserializeOwned, Serialize};

pub(crate) trait Session {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    fn set<T: Serialize>(&self, key: &str, value: T) -> Result<()>;
    fn pop<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>>;
    fn remove(&mut self, key: &str);
}

pub struct ActixSession(actix_session::Session);

impl actix_web::FromRequest for ActixSession {
    type Config = ();
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        {
            ok(ActixSession(
                actix_session::Session::from_request(req, payload)
                    .into_inner()
                    .unwrap(),
            ))
        }
    }
}

impl Session for ActixSession {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        Ok(self
            .0
            .get(key)
            .map_err(|e| eyre!("failed to de-serialize session: {}", e))?)
    }

    fn set<T: Serialize>(&self, key: &str, value: T) -> Result<()> {
        Ok(self
            .0
            .set(key, value)
            .map_err(|e| eyre!("failed to serialize session: {}", e))?)
    }

    fn pop<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>> {
        Ok(if let Some(value) = self.get(key)? {
            self.remove(key);
            Some(value)
        } else {
            None
        })
    }

    fn remove(&mut self, key: &str) {
        self.0.remove(key)
    }
}

#[cfg(test)]
pub mod test {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use eyre::Result;
    use serde::{de::DeserializeOwned, Serialize};

    use super::Session;

    #[derive(Default)]
    pub struct TestSession {
        inner: RefCell<HashMap<String, String>>,
    }

    impl Session for TestSession {
        fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
            Ok(self
                .inner
                .borrow()
                .get(key)
                .map(|val| serde_json::from_str(val))
                .transpose()?)
        }

        fn set<T: Serialize>(&self, key: &str, value: T) -> Result<()> {
            self.inner
                .borrow_mut()
                .insert(key.to_owned(), serde_json::to_string(&value)?);
            Ok(())
        }

        fn pop<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>> {
            Ok(self
                .inner
                .borrow_mut()
                .remove(key)
                .map(|val| serde_json::from_str(&val))
                .transpose()?)
        }

        fn remove(&mut self, key: &str) {
            self.inner.borrow_mut().remove(key);
        }
    }
}
