use std::{future::Future, time::Duration};

use async_trait::async_trait;
use futures::future::{join, select_all};
use glow_events::v2::{Command, Event, Message, Payload};
use tokio::sync::broadcast::channel;

pub type Sender = tokio::sync::broadcast::Sender<Message>;
pub type Receiver = tokio::sync::broadcast::Receiver<Message>;

#[async_trait]
pub trait Handler: Send + Sync {
    async fn run(&self, tx: Sender);
}

#[async_trait]
impl<F, R> Handler for F
where
    F: Fn(Sender) -> R + Clone + Sync + Send + 'static,
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

async fn stopper(tx: Sender) {
    let mut rx = tx.subscribe();

    while let Ok(message) = rx.recv().await {
        if let Payload::Command(Command::Stop) = message.payload() {
            // small delay to allow other handlers to receive the event
            tokio::time::delay_for(Duration::from_millis(50)).await;
            return;
        }
    }
}

impl Runner {
    pub fn add<T: Handler + 'static>(&mut self, handler: T) {
        self.handlers.push(Box::new(handler));
    }

    pub async fn run(self) {
        let (sender, _) = channel(20);

        let mut handlers = self.handlers;
        handlers.insert(0, Box::new(stopper));

        let handler_futures = handlers.iter().map(|handler| handler.run(sender.clone()));

        let sender = sender.clone();
        let start_handler = async move {
            if sender.send(Message::new_event(Event::Started)).is_err() {
                panic!("failed to send start event");
            }
        };

        // start_handler comes second so that handlers can receive
        // the start event.
        join(select_all(handler_futures), start_handler).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glow_events::v2::{Command, Payload};
    use std::time::Duration;

    struct TestHandler {
        sender: Sender,
    }

    #[async_trait]
    impl Handler for TestHandler {
        async fn run(&self, tx: Sender) {
            let mut rx = tx.subscribe();

            while let Ok(message) = rx.recv().await {
                self.sender.send(message).unwrap();
            }
        }
    }

    async fn party_runner(tx: Sender) {
        tx.send(Message::new_command(Command::RunParty)).unwrap();
        tx.send(Message::new_command(Command::RunParty)).unwrap();
        // delay to avoid the runner shutting down
        tokio::time::delay_for(Duration::from_millis(50)).await;
    }

    async fn stopper(tx: Sender) {
        tx.send(Message::new_command(Command::RunParty)).unwrap();
        tx.send(Message::new_command(Command::Stop)).unwrap();
        tokio::time::delay_for(Duration::from_millis(50)).await;
        tx.send(Message::new_command(Command::RunParty)).unwrap();
        // delay to avoid the runner shutting down
        tokio::time::delay_for(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_runner() {
        let mut runner = Runner::default();
        let (tx, mut rx) = channel(3);

        runner.add(TestHandler { sender: tx });
        runner.add(party_runner);

        tokio::spawn(async move {
            runner.run().await;
        });

        assert_eq!(
            rx.recv().await.unwrap().payload(),
            &Payload::Command(Command::RunParty)
        );
        assert_eq!(
            rx.recv().await.unwrap().payload(),
            &Payload::Command(Command::RunParty)
        );
        assert_eq!(
            rx.recv().await.unwrap().payload(),
            &Payload::Event(Event::Started)
        );
        assert!(rx.recv().await.is_err());
    }

    #[tokio::test]
    async fn test_stop_runner_with_event() {
        let mut runner = Runner::default();
        let (tx, mut rx) = channel(3);

        runner.add(TestHandler { sender: tx });
        runner.add(stopper);

        tokio::spawn(async move {
            runner.run().await;
        });

        assert_eq!(
            rx.recv().await.unwrap().payload(),
            &Payload::Command(Command::RunParty)
        );
        assert_eq!(
            rx.recv().await.unwrap().payload(),
            &Payload::Command(Command::Stop)
        );
        assert_eq!(
            rx.recv().await.unwrap().payload(),
            &Payload::Event(Event::Started)
        );
        assert!(rx.recv().await.is_err(),);
    }
}
