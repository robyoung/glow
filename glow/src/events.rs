use std::sync::mpsc::{sync_channel, SyncSender};

use glow_events::{Event, Message};

pub trait EventHandler {
    fn start(&mut self, _sender: SyncSender<Event>) {}
    fn handle(&mut self, _event: &Event, _sender: &SyncSender<Event>) {}
}

pub fn run_loop(mut handlers: Vec<Box<dyn EventHandler>>) {
    let (sender, receiver) = sync_channel(20);

    for handler in handlers.iter_mut() {
        handler.start(sender.clone());
    }

    for event in receiver.iter() {
        for handler in handlers.iter_mut() {
            handler.handle(&event, &sender);
        }
        if let Message::Stop = event.message() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glow_events::{TapEvent};

    struct SendOneSource {}

    impl EventHandler for SendOneSource {
        fn start(&mut self, sender: SyncSender<Event>) {
            sender
                .send(Event::new(Message::Tap(TapEvent::SingleTap)))
                .unwrap();
            sender.send(Event::new(Message::Stop)).unwrap();
        }
    }

    struct StoringEventReceiver {
        events: SyncSender<Event>,
    }

    impl EventHandler for StoringEventReceiver {
        fn handle(&mut self, event: &Event, _: &SyncSender<Event>) {
            self.events.send(event.clone()).unwrap();
        }
    }

    #[test]
    fn run_run_loop() {
        // arrange
        let (sender, receiver) = sync_channel(20);
        let handler = StoringEventReceiver { events: sender };

        // act
        run_loop(vec![Box::new(SendOneSource {}), Box::new(handler)]);

        // assert
        let events = receiver.iter().collect::<Vec<Event>>();

        assert_eq!(events.len(), 2);
        assert_eq!(*events[0].message(), Message::Tap(TapEvent::SingleTap));
        assert_eq!(*events[1].message(), Message::Stop);
    }
}
