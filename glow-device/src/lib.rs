pub mod events;
pub mod leds;
pub mod tplink;

use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread, time,
};

use am2320::AM2320;
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

pub struct EnvironmentSensor {}

const VIBRATION_SENSOR_INTERRUPT_PIN: u8 = 17;
const VIBRATION_SENSOR_INTERRUPT_BOUNCE: u128 = 300;
const ENVIRONMENT_SENSOR_ERROR_LIMIT: u8 = 3;
const ENVIRONMENT_SENSOR_ERROR_BACKOFF_LIMIT: u64 = 3;
const ENVIRONMENT_SENSOR_SLEEP: u64 = 15;
const ENVIRONMENT_SENSOR_MAX_SKIP: u8 = 10;

impl MessageHandler for EnvironmentSensor {
    fn start(&mut self, sender: SyncSender<Message>) {
        thread::spawn(move || {
            let device = I2c::new().expect("could not initialise I2C");
            let delay = Delay::new();

            let mut am2320 = AM2320::new(device, delay);
            let mut previous_data: Option<Measurement> = None;
            let mut num_skipped: u8 = 0;

            loop {
                let measurement = read_am2320(&mut am2320);

                let changed = if let Some(previous_data) = &previous_data {
                    !previous_data.roughly_equal(&measurement)
                } else {
                    true
                };

                if changed || num_skipped > ENVIRONMENT_SENSOR_MAX_SKIP {
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

                thread::sleep(time::Duration::from_secs(ENVIRONMENT_SENSOR_SLEEP));
            }
        });
    }
}

fn read_am2320(sensor: &mut AM2320<I2c, Delay>) -> Measurement {
    let mut error_count: u8 = 0;
    let mut backoff_count: u64 = 0;
    loop {
        match sensor.read() {
            Ok(m) => return Measurement::new(m.temperature, m.humidity),
            Err(err) => {
                error_count += 1;
                if error_count > ENVIRONMENT_SENSOR_ERROR_LIMIT {
                    let sleep = ENVIRONMENT_SENSOR_SLEEP * (backoff_count + 1);
                    error!("too many errors, backing off for {}s: {:?}", sleep, err);
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
    url: String,
    token: String,
    sender: SyncSender<Message>,
    receiver: Option<Receiver<Message>>,
}

impl WebEventHandler {
    pub fn new(url: String, token: String) -> WebEventHandler {
        let (sender, receiver) = sync_channel(20);
        WebEventHandler {
            url,
            token,
            sender,
            receiver: Some(receiver),
        }
    }

    fn send_events(
        client: &ureq::Agent,
        token: &str,
        url: &str,
        events: &Vec<Message>,
    ) -> Option<Vec<Message>> {
        let mut tries = 5;
        while tries > 0 {
            // make request to server
            let resp = client
                .post(url)
                .set("Content-Type", "application/json")
                .auth_kind("Bearer", &token)
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
            tries = tries - 1;
        }
        error!("Failed all attempts at sending events");

        None
    }
}

impl MessageHandler for WebEventHandler {
    fn start(&mut self, sender: SyncSender<Message>) {
        let url = self.url.clone();
        let token = self.token.clone();
        // TODO: think of a better way of doing this, maybe send out on sender
        let receiver = self.receiver.take().unwrap();

        thread::spawn(move || {
            let client = ureq::agent();
            loop {
                // read all events off the queue
                let events = receiver.try_iter().collect::<Vec<Message>>();

                let mut no_messages = events.is_empty();

                // make request to server
                let commands = WebEventHandler::send_events(&client, &token, &url, &events);

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
        });
    }

    fn handle(&mut self, message: &Message, _: &SyncSender<Message>) {
        if let Payload::Event(_) = message.payload() {
            if let Err(err) = self.sender.send(message.clone()) {
                error!("failed to send event to remote worker: {:?}", err);
            }
        }
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
