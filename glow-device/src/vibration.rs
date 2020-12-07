use std::{thread, time};

use log::{error, info};
use rppal::gpio::{Gpio, Trigger};

use crate::events::Sender;
use glow_events::v2::{Event, Message};

const INTERRUPT_PIN: u8 = 17;
const INTERRUPT_BOUNCE: u128 = 300;

pub async fn handler(tx: Sender) {
    let (interrupt_sender, mut interrupt_receiver) = tokio::sync::mpsc::channel(5);

    thread::spawn(move || {
        run_worker(interrupt_sender);
    });

    while interrupt_receiver.recv().await.is_some() {
        tx.send(Message::new_event(Event::SingleTap))
            .expect("Failed to write tap event");
    }
}

type InterruptSender = tokio::sync::mpsc::Sender<()>;

fn run_worker(mut interrupts: InterruptSender) {
    let gpio = Gpio::new().unwrap();
    let mut pin = gpio.get(INTERRUPT_PIN).unwrap().into_input_pullup();
    pin.set_interrupt(Trigger::FallingEdge).unwrap();
    let mut last_event = time::Instant::now();

    loop {
        match pin.poll_interrupt(true, None) {
            Ok(Some(_)) => {
                if last_event.elapsed().as_millis() > INTERRUPT_BOUNCE {
                    last_event = time::Instant::now();
                    if let Err(err) = interrupts.try_send(()) {
                        error!("Failed to write tap event to channel: {:?}", err);
                    }
                }
            }
            Ok(None) => {
                info!("No interrupt to handle");
            }
            Err(err) => {
                error!("Failure detecting tap event: {:?}", err);
            }
        }
    }
}
