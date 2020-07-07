pub mod events;
pub mod leds;
pub mod tplink;

use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread, time,
};

use am2320::Am2320;
use log::{debug, error, info, warn};
use rppal::{
    gpio::{Gpio, Trigger},
    hal::Delay,
    i2c::I2c,
};

use glow_events::{
    v2::{Command, Event, Message, Payload},
    Measurement,
};

use crate::events::MessageHandler;
use crate::leds::{Brightness, Colour, ColourRange, LEDs};

const VIBRATION_SENSOR_INTERRUPT_PIN: u8 = 17;
const VIBRATION_SENSOR_INTERRUPT_BOUNCE: u128 = 300;
const ENVIRONMENT_SENSOR_ERROR_LIMIT: u8 = 3;
const ENVIRONMENT_SENSOR_ERROR_BACKOFF_LIMIT: u64 = 3;
const ENVIRONMENT_SENSOR_SLEEP: u64 = 30;
const ENVIRONMENT_SENSOR_MAX_SKIP: u8 = 10;

/// Read the AM2320 temperature and humidity sensor and emit Measurement events
pub struct EnvironmentSensor {}

struct EnvironmentWorker {
    am2320: Am2320<I2c, Delay>,
}

impl EnvironmentWorker {
    fn new() -> Self {
        let device = I2c::new().expect("could not initialise I2C");
        let delay = Delay::new();

        EnvironmentWorker {
            am2320: Am2320::new(device, delay),
        }
    }

    fn run(&mut self, sender: SyncSender<Message>) {
        let mut previous_data: Option<Measurement> = None;
        let mut num_skipped: u8 = 0;

        loop {
            let measurement = self.read();

            if self.should_send(&measurement, &previous_data, num_skipped) {
                num_skipped = 0;
                debug!(
                    "Sending changed data: {:?} {:?}",
                    measurement, previous_data
                );
                previous_data = Some(measurement);

                let message = Message::event(Event::Measurement(measurement));
                if let Err(err) = sender.send(message) {
                    warn!("Failed to write sensor data to channel: {:?}", err);
                }
            } else {
                num_skipped += 1;
                debug!(
                    "Skipping unchanged data: {:?} {:?}",
                    measurement, previous_data
                );
            }

            self.sleep(num_skipped);
            thread::sleep(time::Duration::from_secs(ENVIRONMENT_SENSOR_SLEEP));
        }
    }

    fn read(&mut self) -> Measurement {
        let mut error_count: u8 = 0;
        let mut backoff_count: u64 = 0;
        loop {
            match self.am2320.read() {
                Ok(m) => {
                    if error_count > 0 {
                        info!(
                            "AM2320 read success after {} failures: {:?} ",
                            error_count, m
                        );
                    }
                    return m.into();
                }
                Err(err) => {
                    error!("AM232O read error: {:?}", err);
                    error_count += 1;
                    if error_count > ENVIRONMENT_SENSOR_ERROR_LIMIT {
                        let sleep = ENVIRONMENT_SENSOR_SLEEP * (backoff_count + 1);
                        error!("too many errors, backing off for {}s", sleep);
                        thread::sleep(time::Duration::from_secs(sleep));
                        error_count = 0;
                        if backoff_count < ENVIRONMENT_SENSOR_ERROR_BACKOFF_LIMIT {
                            backoff_count += 1;
                        } else {
                            error!("environment sensor backoff limit reached; shutting down");
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }

    fn should_send(
        &self,
        measurement: &Measurement,
        previous_data: &Option<Measurement>,
        num_skipped: u8,
    ) -> bool {
        let is_changed = if let Some(previous_data) = previous_data {
            !previous_data.temperature_roughly_equal(measurement)
        } else {
            true
        };
        is_changed || num_skipped > ENVIRONMENT_SENSOR_MAX_SKIP
    }

    fn sleep(&self, num_skipped: u8) {
        thread::sleep(time::Duration::from_secs(
            (ENVIRONMENT_SENSOR_SLEEP as f64
                + ENVIRONMENT_SENSOR_SLEEP as f64 * 0.1 * num_skipped as f64) as u64,
        ));
    }
}

impl MessageHandler for EnvironmentSensor {
    fn start(&mut self, sender: SyncSender<Message>) {
        thread::spawn(move || {
            let mut worker = EnvironmentWorker::new();
            worker.run(sender);
        });
    }
}

/// Translate interrupts from the vibration sensor into tap events.
pub struct VibrationSensor {}

impl MessageHandler for VibrationSensor {
    fn start(&mut self, sender: SyncSender<Message>) {
        let gpio = Gpio::new().unwrap();
        let mut pin = gpio
            .get(VIBRATION_SENSOR_INTERRUPT_PIN)
            .unwrap()
            .into_input_pullup();
        pin.set_interrupt(Trigger::FallingEdge).unwrap();
        thread::spawn(move || {
            let mut last_event = time::Instant::now();
            loop {
                match pin.poll_interrupt(true, None) {
                    Ok(Some(_)) => {
                        if last_event.elapsed().as_millis() > VIBRATION_SENSOR_INTERRUPT_BOUNCE {
                            last_event = time::Instant::now();
                            if let Err(err) = sender.send(Message::event(Event::SingleTap)) {
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
        });
    }
}

pub struct PrintMeasurementHandler;

impl PrintMeasurementHandler {
    fn print(&self, message: &Message, name: &str, temperature: f64, humidity: f64) {
        println!(
            "{},{},{},{}",
            message.stamp().to_rfc3339(),
            name,
            temperature,
            humidity
        );
    }
}

impl MessageHandler for PrintMeasurementHandler {
    fn handle(&mut self, message: &Message, _: &SyncSender<Message>) {
        match message.payload() {
            Payload::Event(Event::Measurement(measurement)) => self.print(
                message,
                "data",
                measurement.temperature,
                measurement.humidity,
            ),
            Payload::Event(Event::SingleTap) => self.print(message, "tap", 0.0, 0.0),
            _ => {}
        }
    }
}

/// Control the colour LED strip
pub struct LEDHandler {
    leds: Box<dyn LEDs>,
    colour_range: ColourRange,
    colours: Vec<Colour>,
    brightness: f32,
}

impl LEDHandler {
    pub fn new(leds: impl LEDs + 'static, colour_range: ColourRange) -> Self {
        let colours = colour_range.all(Colour::black());
        Self {
            leds: Box::new(leds),
            colour_range,
            colours,
            brightness: Brightness::default().value(),
        }
    }
}

impl MessageHandler for LEDHandler {
    fn handle(&mut self, message: &Message, sender: &SyncSender<Message>) {
        match message.payload() {
            Payload::Event(Event::Measurement(measurement)) => {
                let colours = self.colour_range.get_pixels(measurement.temperature as f32);
                if colours.iter().zip(&self.colours).any(|(&a, &b)| a != b) {
                    self.colours = colours;
                    sender.send(Message::command(Command::UpdateLEDs)).unwrap();
                } else {
                    debug!("Not updating unchanged LEDs");
                }
            }
            Payload::Event(Event::SingleTap) => {
                self.brightness = Brightness::next_from(self.brightness).value();
                sender.send(Message::command(Command::RunParty)).unwrap();
                sender.send(Message::command(Command::UpdateLEDs)).unwrap();
            }
            Payload::Command(Command::RunParty) => {
                if let Err(err) = self.leds.party() {
                    error!("party error: {}", err);
                }
            }
            Payload::Command(Command::UpdateLEDs) => {
                if let Err(err) = self.leds.show(&self.colours, self.brightness) {
                    error!("show error: {}", err);
                } else {
                    let colours = self.colours.iter().map(|c| (c.0, c.1, c.2)).collect();
                    sender
                        .send(Message::event(Event::LEDColours(colours)))
                        .unwrap();
                }
            }
            Payload::Command(Command::SetBrightness(brightness)) => {
                self.brightness = *brightness;
                sender.send(Message::command(Command::UpdateLEDs)).unwrap();
                sender
                    .send(Message::event(Event::LEDBrightness(*brightness)))
                    .unwrap();
            }
            _ => {}
        }
    }
}

pub struct WebEventHandler {
    sender: SyncSender<Message>,
    worker: Option<WebEventWorker>,
}

impl WebEventHandler {
    pub fn new(url: String, token: String) -> Self {
        let (sender, receiver) = sync_channel(20);
        Self {
            sender,
            worker: Some(WebEventWorker::new(url, token, receiver)),
        }
    }
}

impl MessageHandler for WebEventHandler {
    fn start(&mut self, sender: SyncSender<Message>) {
        let mut worker = self.worker.take().unwrap();

        thread::spawn(move || worker.run(sender));
    }

    fn handle(&mut self, message: &Message, _: &SyncSender<Message>) {
        if let Payload::Event(_) = message.payload() {
            if let Err(err) = self.sender.send(message.clone()) {
                error!("failed to send event to remote worker: {:?}", err);
            }
        }
    }
}

struct WebEventWorker {
    url: String,
    token: String,
    receiver: Receiver<Message>,
}

impl WebEventWorker {
    fn new(url: String, token: String, receiver: Receiver<Message>) -> Self {
        Self {
            url,
            token,
            receiver,
        }
    }

    fn run(&mut self, sender: SyncSender<Message>) {
        let client = ureq::agent();
        loop {
            // read all events off the queue
            let events = self.get_events_from_queue();

            let mut no_messages = events.is_empty();

            // make request to server
            debug!("sending {} events", events.len());
            let commands = self.send_events(&client, &events);

            if let Some(commands) = commands {
                no_messages = no_messages && commands.is_empty();
                if !commands.is_empty() {
                    info!("received {} commands from remote", commands.len());
                }
                for command in commands {
                    if let Err(err) = sender.send(command) {
                        error!("failed to send remote error to bus {:?}", err);
                    }
                }
            }

            // sleep for poll interval
            let sleep = if no_messages { 5 } else { 1 };
            thread::sleep(time::Duration::from_secs(sleep));
        }
    }

    fn get_events_from_queue(&mut self) -> Vec<Message> {
        self.receiver.try_iter().collect::<Vec<Message>>()
    }

    fn send_events(&self, client: &ureq::Agent, events: &[Message]) -> Option<Vec<Message>> {
        let mut tries = 5;
        while tries > 0 {
            // make request to server
            let resp = client
                .post(&self.url)
                .set("Content-Type", "application/json")
                .auth_kind("Bearer", &self.token)
                .send_json(serde_json::to_value(&events).unwrap());

            if resp.ok() {
                if let Ok(data) = resp.into_json() {
                    if let Ok(commands) = serde_json::from_value::<Vec<Message>>(data) {
                        return Some(commands);
                    } else {
                        error!("received badly formatted json");
                    }
                } else {
                    error!("received invalid json");
                }
            } else {
                error!("Failed to send {} events: {}", events.len(), resp.status());
            }
            tries -= 1;
        }
        error!("Failed all attempts at sending events");

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_roughly_equal_when_within_limits() {
        // arrange
        let previous_data = Measurement {
            temperature: 12.3001,
            humidity: 13.4001,
        };
        let new_data = Measurement {
            temperature: 12.3002,
            humidity: 13.4001,
        };

        // assert
        assert!((&previous_data).roughly_equal(&new_data));
    }

    #[test]
    fn data_is_not_roughly_equal_when_outside_limits() {
        // arrange
        let previous_data = Measurement {
            temperature: 12.3001,
            humidity: 13.4001,
        };
        let new_data = Measurement {
            temperature: 12.4012,
            humidity: 13.4001,
        };

        // assert
        assert!(!(&previous_data).roughly_equal(&new_data));
    }
}
