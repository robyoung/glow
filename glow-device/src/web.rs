use reqwest::Client;

use crate::events::{Handler, Receiver, Sender};
use log::{error, info};

use async_trait::async_trait;
use glow_events::v2::Message;
use std::time::Duration;
use tokio::time::delay_for;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

pub struct WebHandler {
    url: String,
    token: String,
}

impl WebHandler {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }

    async fn send_messages(&self, client: &Client, messages: &[Message]) -> Option<Vec<Message>> {
        let mut tries = 5;
        while tries > 0 {
            tries -= 1;
            let resp = client
                .post(&self.url)
                .bearer_auth(&self.token)
                .json(&serde_json::to_value(&messages).unwrap())
                .send();
            match resp.await {
                Ok(resp) => {
                    if let Ok(data) = resp.json().await {
                        if let Ok(commands) = serde_json::from_value::<Vec<Message>>(data) {
                            return Some(commands);
                        } else {
                            error!("received badly formatted json");
                        }
                    } else {
                        error!("received invalid json");
                    }
                }
                Err(err) => {
                    error!("Failed to send {} events: {}", messages.len(), err);
                }
            }
        }
        error!("Failed all attempts at sending events");

        None
    }
}

#[async_trait]
impl Handler for WebHandler {
    async fn run(&self, tx: Sender) {
        let client = Client::builder()
            .user_agent(APP_USER_AGENT)
            .build()
            .unwrap();
        let mut rx = tx.subscribe();
        loop {
            // try_recv to get all pending events
            let messages = get_messages_from_queue(&mut rx);
            let mut no_messages = messages.is_empty();

            let commands = self.send_messages(&client, &messages).await;

            if let Some(commands) = commands {
                no_messages = no_messages && commands.is_empty();
                if !commands.is_empty() {
                    info!("received {} commands from remote", commands.len());
                }
                for command in commands {
                    if let Err(err) = tx.send(command) {
                        error!("failed to send remote error to bus {:?}", err);
                    }
                }
            }

            let sleep = if no_messages { 5 } else { 1 };
            delay_for(Duration::from_secs(sleep)).await;
        }
    }
}

fn get_messages_from_queue(rx: &mut Receiver) -> Vec<Message> {
    let mut messages = vec![];
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}
