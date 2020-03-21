use std::sync::mpsc::{sync_channel, SyncSender};

use glow_events::v2::{Command, Event, Message, Payload};

pub trait MessageHandler {
    fn start(&mut self, _sender: SyncSender<Message>) {}
    fn handle(&mut self, message: &Message, sender: &SyncSender<Message>) {
        match message.payload() {
            Payload::Event(event) => self.handle_event(event, sender),
            Payload::Command(command) => self.handle_command(command, sender),
        }
    }
    fn handle_event(&mut self, _event: &Event, _sender: &SyncSender<Message>) {}
    fn handle_command(&mut self, _command: &Command, _sender: &SyncSender<Message>) {}
}

pub fn run_loop(mut handlers: Vec<Box<dyn MessageHandler>>) {
    let (sender, receiver) = sync_channel(20);

    for handler in handlers.iter_mut() {
        handler.start(sender.clone());
    }

    sender
        .send(Message::event(Event::Started))
        .expect("could not send startup event");

    for message in receiver.iter() {
        for handler in handlers.iter_mut() {
            handler.handle(&message, &sender);
        }
        if let Payload::Command(Command::Stop) = message.payload() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SendOneSource {}

    impl MessageHandler for SendOneSource {
        fn start(&mut self, sender: SyncSender<Message>) {
            sender.send(Message::event(Event::SingleTap)).unwrap();
            sender.send(Message::command(Command::Stop)).unwrap();
        }
    }

    struct StoringReceiver {
        messages: SyncSender<Message>,
    }

    impl MessageHandler for StoringReceiver {
        fn handle(&mut self, message: &Message, _: &SyncSender<Message>) {
            self.messages.send(message.clone()).unwrap();
        }
    }

    #[test]
    fn run_run_loop() {
        // arrange
        let (sender, receiver) = sync_channel(20);
        let handler = StoringReceiver { messages: sender };

        // act
        run_loop(vec![Box::new(SendOneSource {}), Box::new(handler)]);

        // assert
        let messages = receiver.iter().collect::<Vec<Message>>();

        assert_eq!(messages.len(), 2);
        assert_eq!(*messages[0].payload(), Payload::Event(Event::SingleTap));
        assert_eq!(*messages[1].payload(), Payload::Command(Command::Stop));
    }
}
