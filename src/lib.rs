extern crate am2320;
extern crate blinkt;
extern crate chrono;
#[macro_use]
extern crate log;

pub mod events;
pub mod leds;

use std::{collections::HashMap, sync::mpsc::SyncSender, thread, time};

use am2320::AM2320;

use chrono::{offset::Utc, DateTime};
use rppal::{
    gpio::{Gpio, Trigger},
    hal::Delay,
    i2c::I2c,
};

use reqwest;

use crate::events::{
    EnvironmentEvent, Event, EventHandler, EventSource, LEDEvent, Measurement, Message, TapEvent,
};
use crate::leds::{Colour, ColourRange, LEDs, LedBrightness, StaticLedBrightness};

pub struct EnvironmentSensor {}

const VIBRATION_SENSOR_INTERRUPT_PIN: u8 = 17;
const VIBRATION_SENSOR_INTERRUPT_BOUNCE: u128 = 300;
const ENVIRONMENT_SENSOR_ERROR_LIMIT: u8 = 3;
const ENVIRONMENT_SENSOR_ERROR_BACKOFF_LIMIT: u64 = 3;
const ENVIRONMENT_SENSOR_SLEEP: u64 = 5;

impl EventSource for EnvironmentSensor {
    fn start(&self, sender: SyncSender<Event>) {
        thread::spawn(move || {
            let device = I2c::new().expect("could not initialise I2C");
            let delay = Delay::new();

            let mut am2320 = AM2320::new(device, delay);
            let mut previous_data: Option<Measurement> = None;

            loop {
                let measurement = read_am2320(&mut am2320);

                let changed = if let Some(previous_data) = &previous_data {
                    if measurement_is_roughly_equal(previous_data, &measurement) {
                        debug!("Skipping unchanged data");
                        false
                    } else {
                        true
                    }
                } else {
                    true
                };

                if changed {
                    previous_data = Some(clone_measurement(&measurement));

                    let event =
                        Event::new_measurement(measurement.temperature, measurement.humidity);
                    if let Err(err) = sender.send(event) {
                        warn!("Failed to write sensor data to channel: {:?}", err);
                    }
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

fn measurement_is_roughly_equal(previous_data: &Measurement, new_data: &Measurement) -> bool {
    (previous_data.temperature - new_data.temperature).abs() < 0.001
        && (previous_data.humidity - new_data.humidity).abs() < 0.001
}

fn clone_measurement(measurement: &Measurement) -> Measurement {
    Measurement {
        temperature: measurement.temperature,
        humidity: measurement.humidity,
    }
}

fn measurement_as_map(stamp: DateTime<Utc>, measurement: &Measurement) -> HashMap<&str, String> {
    let mut result = HashMap::new();
    result.insert("value1", stamp.to_rfc3339());
    result.insert("value2", format!("{}", measurement.temperature));
    result.insert("value3", format!("{}", measurement.humidity));
    result
}

pub struct VibrationSensor {}

impl EventSource for VibrationSensor {
    fn start(&self, sender: SyncSender<Event>) {
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

pub struct LEDHandler {
    leds: Box<dyn LEDs>,
    colour_range: ColourRange,
    colours: Vec<Colour>,
    brightness: Box<dyn LedBrightness>,
}

impl LEDHandler {
    pub fn new(leds: impl LEDs + 'static, colour_range: ColourRange) -> Self {
        Self::new_with_brightness(leds, colour_range, StaticLedBrightness::Dim)
    }

    pub fn new_with_brightness(
        leds: impl LEDs + 'static,
        colour_range: ColourRange,
        brightness: impl LedBrightness + 'static,
    ) -> Self {
        let colours = colour_range.all(Colour::black());
        Self {
            leds: Box::new(leds),
            colour_range,
            colours,
            brightness: Box::new(brightness),
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
                self.brightness.next();
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
                if let Err(err) = self.leds.show(&self.colours, self.brightness.value()) {
                    error!("show error: {}", err);
                }
            }
            _ => {}
        }
    }
}

const WEB_HOOK_PREVIOUS_VALUES: usize = 40;

pub struct WebHookHandler {
    client: reqwest::Client,
    url: String,
    last_send: time::Instant,
    last_value: Option<Measurement>,
    previous_values: [Option<Measurement>; WEB_HOOK_PREVIOUS_VALUES],
}

impl WebHookHandler {
    pub fn new(url: String) -> WebHookHandler {
        WebHookHandler {
            client: reqwest::Client::new(),
            url,
            last_send: time::Instant::now() - time::Duration::from_secs(100_000),
            last_value: None,
            previous_values: [None; WEB_HOOK_PREVIOUS_VALUES],
        }
    }

    fn should_send(&mut self, measurement: Measurement) -> bool {
        let should_send = if self.last_value.is_none() {
            // we have not sent a value yet
            true
        } else if self.last_value.unwrap() == measurement {
            // current value is the same as the last one sent
            false
        } else if self.last_send.elapsed() < time::Duration::from_secs(60) {
            // we already sent a value less than 60 seconds ago
            false
        } else {
            // more than half of the previous values are different to the last sent one
            self.previous_values
                .iter()
                .filter(|value| match value {
                    None => false,
                    Some(value) => self.last_value.unwrap().temperature != (*value).temperature,
                })
                .count() as f64
                / WEB_HOOK_PREVIOUS_VALUES as f64
                > 0.9
        };

        // push the new value
        self.previous_values.rotate_right(1);
        self.previous_values[0] = Some(measurement);

        if should_send {
            self.last_value = Some(measurement);
            self.last_send = time::Instant::now();
        }

        should_send
    }
}

impl EventHandler for WebHookHandler {
    fn handle(&mut self, event: &Event, _sender: &SyncSender<Event>) {
        if let Message::Environment(EnvironmentEvent::Measurement(measurement)) = event.message() {
            if self.should_send(*measurement) {
                let payload = measurement_as_map(event.stamp(), measurement);
                debug!("IFTTT payload {:?}", payload);
                match self.client.post(self.url.as_str()).json(&payload).build() {
                    Ok(request) => {
                        debug!("IFTTT request {:?}", request);
                        if let Err(err) = self.client.execute(request) {
                            error!("Failed to send to IFTTT: {:?}", err);
                        }
                    }
                    Err(err) => {
                        error!("Failed to build IFTTT request: {:?}", err);
                    }
                }
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
        assert!(measurement_is_roughly_equal(&previous_data, &new_data));
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
        assert!(!measurement_is_roughly_equal(&previous_data, &new_data));
    }
}
