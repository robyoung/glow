use std::future::Future;

use async_trait::async_trait;
use futures::future::{join, select_all};
use glow_events::v2::{Event, Message};
use tokio::sync::broadcast::channel;

pub type Sender = tokio::sync::broadcast::Sender<Message>;
pub type Receiver = tokio::sync::broadcast::Receiver<Message>;

#[async_trait]
pub trait Handler {
    async fn run(&self, tx: Sender);
}

#[async_trait]
impl<F, R> Handler for F
where
    F: Fn(Sender) -> R + Clone + Sync + 'static,
    R: Future<Output = ()> + Send + 'static,
{
    async fn run(&self, tx: Sender) {
        (self)(tx).await
    }
}

#[derive(Default)]
pub struct Runner {
    handlers: Vec<Box<dyn Handler>>,
}

impl Runner {
    pub fn add<T: Handler + 'static>(&mut self, handler: T) {
        self.handlers.push(Box::new(handler));
    }

    pub async fn run(self) {
        let (sender, _) = channel(20);

        let handler_futures = self
            .handlers
            .iter()
            .map(|handler| handler.run(sender.clone()));

        let sender = sender.clone();
        let start_handler = async move {
            if sender.send(Message::new_event(Event::Started)).is_err() {
                panic!("failed to send start event");
            }
        };

        join(select_all(handler_futures), start_handler).await;
    }
}

#[cfg(test)]
mod tests {
    // TODO: test async runner
}
