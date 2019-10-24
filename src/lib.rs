extern crate am2320;
extern crate blinkt;
extern crate chrono;

pub mod leds;
pub mod events;

use std::{
    collections::HashMap,
    sync::mpsc::SyncSender,
    thread, time,
};

use am2320::AM2320;

use chrono::{offset::Utc, DateTime};
use rppal::{
    gpio::{Gpio, Trigger},
    hal::Delay,
    i2c::I2c,
};

use reqwest;

use crate::leds::{Colour, ColourRange, LEDs, LedBrightness};
use crate::events::{Event, Message, Measurement, EventHandler};

static ERROR_LIMIT: u8 = 3;
static INTERRUPT_PIN: u8 = 17;
static INTERRUPT_BOUNCE: u128 = 300;


pub fn start_environment_sensor(sender: SyncSender<Event>) {
    thread::spawn(move || {
        let device = I2c::new().expect("could not initialise I2C");
        let delay = Delay::new();

        let mut am2320 = AM2320::new(device, delay);
        let mut previous_data: Option<Measurement> = None;

        loop {
            let event = match read_am2320(&mut am2320) {
                Some(measurement) => {
                    let unchanged = if let Some(previous_data) = &previous_data {
                        if measurement_is_roughly_equal(previous_data, &measurement) {
                            eprintln!("Skipping unchanged data");
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if unchanged {
                        None
                    } else {
                        previous_data = Some(clone_measurement(&measurement));

                        Some(Event::new_envirornment(measurement.temperature, measurement.humidity))
                    }
                }
                None => None,
            };

            if let Some(event) = event {
                if let Err(err) = sender.send(event) {
                    eprintln!("Failed to write sensor data to channel: {:?}", err);
                }
            }

            thread::sleep(time::Duration::from_secs(30));
        }
    });
}

fn read_am2320(sensor: &mut AM2320<I2c, Delay>) -> Option<Measurement> {
    let mut error_count: u8 = 0;
    loop {
        match sensor.read() {
            Ok(m) => return Some(Measurement::new(m.temperature, m.humidity)),
            Err(err) => {
                error_count += 1;
                eprintln!("Failed to read AM2320: {:?}", err);
                if error_count > ERROR_LIMIT {
                    eprintln!("too many errors, failing");
                    return None;
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

pub fn start_vibration_sensor(sender: SyncSender<Event>) {
    let gpio = Gpio::new().unwrap();
    let mut pin = gpio.get(INTERRUPT_PIN).unwrap().into_input_pullup();
    pin.set_interrupt(Trigger::FallingEdge).unwrap();
    thread::spawn(move || {
        let mut last_event = time::Instant::now();
        loop {
            match pin.poll_interrupt(true, None) {
                Ok(Some(_)) => {
                    if last_event.elapsed().as_millis() > INTERRUPT_BOUNCE {
                        last_event = time::Instant::now();
                        if let Err(err) = sender.send(Event::new(Message::TapEvent)) {
                            eprintln!("Failed to write tap event to channel: {:?}", err);
                        }
                    }
                }

                Ok(None) => {
                    eprintln!("No interrupt to handle");
                }

                Err(err) => {
                    eprintln!("Failure detecting tap event: {:?}", err);
                }
            }
        }
    });
}

pub struct PrintMeasurementHandler;

impl PrintMeasurementHandler {
    fn print(&self, event: &Event, name: &str, temperature: f64, humidity: f64) {
        println!("{},{},{},{}", event.stamp().to_rfc3339(), name, temperature, humidity);
    }
}

impl EventHandler for PrintMeasurementHandler {
    fn handle(&mut self, event: &Event, _: &SyncSender<Event>) {
        match event.message() {
            Message::Environment(measurement) => self.print(
                event, "data", 
                measurement.temperature,
                measurement.humidity,
            ),
            Message::TapEvent => self.print(
                event, "tap", 0.0, 0.0
            ),
            _ => {},
        }
    }
}

pub struct LEDHandler {
    leds: Box<dyn LEDs>,
    colour_range: ColourRange,
    colours: Vec<Colour>,
    brightness: LedBrightness,
}

impl LEDHandler {
    pub fn new(leds: impl LEDs + 'static, colour_range: ColourRange) -> LEDHandler {
        let colours = colour_range.all(Colour::black());
        LEDHandler {
            leds: Box::new(leds),
            colour_range,
            colours,
            brightness: LedBrightness::Dim,
        }
    }
}

impl EventHandler for LEDHandler {
    fn handle(&mut self, event: &Event, sender: &SyncSender<Event>) {
        let message = event.message();
        match message {
            Message::Environment(measurement) => {
                self.colours = self.colour_range.get_pixels(measurement.temperature as f32);
                sender.send(Event::new(Message::UpdateLEDs)).unwrap();
            },
            Message::TapEvent => {
                self.brightness = self.brightness.next();
                sender.send(Event::new(Message::LEDParty)).unwrap();
                sender.send(Event::new(Message::UpdateLEDs)).unwrap();
            },
            Message::LEDParty => {
                if let Err(err) = self.leds.party() {
                    eprintln!("party error: {}", err);
                }
            },
            Message::UpdateLEDs => {
                if let Err(err) = self.leds.show(&self.colours, self.brightness.value()) {
                    eprintln!("show error: {}", err);
                }
            },
            _ => {},
        }
    }
}

pub struct WebHookHandler {
    client: reqwest::Client,
    url: String,
    last_send: time::Instant,
}

impl WebHookHandler {
    pub fn new(url: String) -> WebHookHandler {
        WebHookHandler {
            client: reqwest::Client::new(),
            url,
            last_send: time::Instant::now() - time::Duration::from_secs(100000),
        }
    }
}

impl EventHandler for WebHookHandler {
    fn handle(&mut self, event: &Event, _sender: &SyncSender<Event>) {
        if let Message::Environment(measurement) = event.message() {
            if self.last_send < time::Instant::now() - time::Duration::from_secs(60 * 30) {
                let payload = measurement_as_map(event.stamp(), measurement);
                eprintln!("IFTTT payload {:?}", payload);
                match self.client.post(self.url.as_str()).json(&payload).build() {
                    Ok(request) => {
                        eprintln!("IFTTT request {:?}", request);
                        if let Err(err) = self.client.execute(request) {
                            eprintln!("Failed to send to IFTTT: {:?}", err);
                        }
                    },
                    Err(err) => {
                        eprintln!("Failed to build IFTTT request: {:?}", err);
                    }
                }
                self.last_send = time::Instant::now();
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
