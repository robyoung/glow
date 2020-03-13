extern crate am2320;
extern crate blinkt;
extern crate chrono;
#[macro_use]
extern crate log;
extern crate ureq;
extern crate glow_events;

pub mod events;
pub mod leds;

use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread, time,
};

use am2320::AM2320;
use rppal::{
    gpio::{Gpio, Trigger},
    hal::Delay,
    i2c::I2c,
};
use serde_json::json;

use glow_events::{EnvironmentEvent, Event, LEDEvent, Measurement, Message, TapEvent};

use crate::events::EventHandler;
use crate::leds::{Brightness, Colour, ColourRange, LEDs};

pub struct EnvironmentSensor {}

const VIBRATION_SENSOR_INTERRUPT_PIN: u8 = 17;
const VIBRATION_SENSOR_INTERRUPT_BOUNCE: u128 = 300;
const ENVIRONMENT_SENSOR_ERROR_LIMIT: u8 = 3;
const ENVIRONMENT_SENSOR_ERROR_BACKOFF_LIMIT: u64 = 3;
const ENVIRONMENT_SENSOR_SLEEP: u64 = 15;

impl EventHandler for EnvironmentSensor {
    fn start(&mut self, sender: SyncSender<Event>) {
        thread::spawn(move || {
            let device = I2c::new().expect("could not initialise I2C");
            let delay = Delay::new();

            let mut am2320 = AM2320::new(device, delay);
            let mut previous_data: Option<Measurement> = None;

            loop {
                let measurement = read_am2320(&mut am2320);

                let changed = if let Some(previous_data) = &previous_data {
                    !previous_data.roughly_equal(&measurement)
                } else {
                    true
                };

                if changed {
                    debug!(
                        "Sending changed data: {:?} {:?}",
                        measurement, previous_data
                    );
                    previous_data = Some(measurement);

                    let event =
                        Event::new_measurement(measurement.temperature, measurement.humidity);
                    if let Err(err) = sender.send(event) {
                        warn!("Failed to write sensor data to channel: {:?}", err);
                    }
                } else {
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

impl EventHandler for VibrationSensor {
    fn start(&mut self, sender: SyncSender<Event>) {
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
                            if let Err(err) =
                                sender.send(Event::new(Message::Tap(TapEvent::SingleTap)))
                            {
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
    fn print(&self, event: &Event, name: &str, temperature: f64, humidity: f64) {
        println!(
            "{},{},{},{}",
            event.stamp().to_rfc3339(),
            name,
            temperature,
            humidity
        );
    }
}

impl EventHandler for PrintMeasurementHandler {
    fn handle(&mut self, event: &Event, _: &SyncSender<Event>) {
        match event.message() {
            Message::Environment(EnvironmentEvent::Measurement(measurement)) => {
                self.print(event, "data", measurement.temperature, measurement.humidity)
            }
            Message::Tap(TapEvent::SingleTap) => self.print(event, "tap", 0.0, 0.0),
            _ => {}
        }
    }
}

pub struct LEDBrightnessHandler {
    brightness: f32,
}

impl LEDBrightnessHandler {
    pub fn new() -> Self {
        Self {
            brightness: Brightness::default().value(),
        }
    }
}

impl EventHandler for LEDBrightnessHandler {
    fn handle(&mut self, event: &Event, sender: &SyncSender<Event>) {
        match event.message() {
            Message::LED(LEDEvent::Brightness(brightness)) => if *brightness != self.brightness {
                self.brightness = *brightness;
            }
            Message::Tap(TapEvent::SingleTap) => {
                sender
                    .send(Event::new(Message::LED(LEDEvent::Brightness(
                        Brightness::next_from(self.brightness).value(),
                    ))))
                    .unwrap();
            }
            _ => {}
        }
    }
}

pub struct LEDHandler {
    leds: Box<dyn LEDs>,
    colour_range: ColourRange,
    colours: Vec<Colour>,
    brightness: Option<f32>,
}

impl LEDHandler {
    pub fn new(leds: impl LEDs + 'static, colour_range: ColourRange) -> Self {
        let colours = colour_range.all(Colour::black());
        Self {
            leds: Box::new(leds),
            colour_range,
            colours,
            brightness: None,
        }
    }
}

impl EventHandler for LEDHandler {
    fn handle(&mut self, event: &Event, sender: &SyncSender<Event>) {
        let message = event.message();
        match message {
            Message::Environment(EnvironmentEvent::Measurement(measurement)) => {
                self.colours = self.colour_range.get_pixels(measurement.temperature as f32);
                sender
                    .send(Event::new(Message::LED(LEDEvent::Update)))
                    .unwrap();
            }
            Message::Tap(TapEvent::SingleTap) => {
                sender
                    .send(Event::new(Message::LED(LEDEvent::Party)))
                    .unwrap();
                sender
                    .send(Event::new(Message::LED(LEDEvent::Update)))
                    .unwrap();
            }
            Message::LED(LEDEvent::Party) => {
                if let Err(err) = self.leds.party() {
                    error!("party error: {}", err);
                }
            }
            Message::LED(LEDEvent::Update) => {
                if let Some(brightness) = self.brightness {
                    if let Err(err) = self.leds.show(&self.colours, brightness) {
                        error!("show error: {}", err);
                    }
                }
            }
            Message::LED(LEDEvent::Brightness(brightness)) => {
                self.brightness = Some(*brightness);
            }
            _ => {}
        }
    }
}

pub struct WebEventHandler {
    url: String,
    token: String,
    sender: SyncSender<Event>,
    receiver: Option<Receiver<Event>>,
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
}

impl EventHandler for WebEventHandler {
    fn start(&mut self, sender: SyncSender<Event>) {
        let url = self.url.clone();
        let token = self.token.clone();
        // TODO: think of a better way of doing this, maybe send out on sender
        let receiver = self.receiver.take().unwrap();

        thread::spawn(move || {
            let client = ureq::agent();
            loop {
                // read all events off the queue
                let events = receiver.try_iter().collect::<Vec<Event>>();
                let mut no_events = events.is_empty();

                // make request to server
                let resp = client
                    .post(url.as_str())
                    .set("Content-Type", "application/json")
                    .auth_kind("Bearer", &token)
                    .send_json(serde_json::to_value(&events).unwrap());

                // send received events on bus
                if resp.ok() {
                    if let Ok(data) = resp.into_json() {
                        if let Ok(events) = serde_json::from_value::<Vec<Event>>(data) {
                            no_events = no_events && events.is_empty();
                            for event in events {
                                if let Err(err) = sender.send(event) {
                                    error!("failed to send remote error to bus {:?}", err);
                                }
                            }
                        } else {
                            error!("received badly formatted json");
                        }
                    } else {
                        error!("received invalid json");
                    }
                } else {
                    error!("Failed to send {} events: {}", events.len(), resp.status());
                }

                // sleep for poll interval
                let sleep = if no_events { 5 } else { 1 };
                thread::sleep(time::Duration::from_secs(sleep));
            }
        });
    }

    fn handle(&mut self, event: &Event, _: &SyncSender<Event>) {
        if let Err(err) = self.sender.send(event.clone()) {
            error!("failed to send event to remote worker: {:?}", err);
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
