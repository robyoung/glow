extern crate am2320;
extern crate blinkt;
extern crate chrono;

pub mod leds;

use std::sync::mpsc::{Receiver, SyncSender};
use std::{thread, time};

use am2320::{Measurement, AM2320};

use chrono::{offset::Utc, DateTime};
use rppal::{
    gpio::{Gpio, Trigger},
    hal::Delay,
    i2c::I2c,
};

use crate::leds::{Colour, ColourRange, LEDs, LedBrightness};

static ERROR_LIMIT: u8 = 3;
static INTERRUPT_PIN: u8 = 17;
static INTERRUPT_BOUNCE: u128 = 300;

pub struct Event {
    stamp: DateTime<Utc>,
    message: Message,
}

impl Event {
    pub fn new(message: Message) -> Event {
        Event {
            stamp: Utc::now(),
            message,
        }
    }

    pub fn stamp(&self) -> DateTime<Utc> {
        self.stamp
    }

    pub fn message(&self) -> &Message {
        &self.message
    }
}

pub enum Message {
    Environment(Measurement),
    TapEvent,
}

pub fn start_environment_sensor(sender: SyncSender<Event>, loop_sleep: u64) {
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

                        Some(Event::new(Message::Environment(measurement)))
                    }
                }
                None => None,
            };

            if let Some(event) = event {
                if let Err(err) = sender.send(event) {
                    eprintln!("Failed to write sensor data to channel: {:?}", err);
                }
            }

            thread::sleep(time::Duration::from_secs(loop_sleep));
        }
    });
}

fn read_am2320(sensor: &mut AM2320<I2c, Delay>) -> Option<Measurement> {
    let mut error_count: u8 = 0;
    loop {
        match sensor.read() {
            Ok(measurement) => return Some(measurement),
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

pub fn main_loop(
    events: Receiver<Event>,
    mut leds: impl LEDs,
    colour_range: ColourRange,
) -> Result<(), String> {
    let mut led_brightness = LedBrightness::Dim;
    let mut pixels = colour_range.all(Colour::black());
    for event in events.iter() {
        match event.message() {
            Message::Environment(measurement) => {
                // print csv
                println!(
                    "{},data,{},{}",
                    event.stamp.to_rfc3339(),
                    measurement.temperature,
                    measurement.humidity
                );

                // calculate pixels
                pixels = colour_range.get_pixels(measurement.temperature as f32);
            }
            Message::TapEvent => {
                led_brightness = led_brightness.next();
                println!("{},tap,0,0", event.stamp.to_rfc3339());
            }
        }
        // update Blinkt
        leds.show(&pixels, led_brightness.value())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::leds::ColourBucket;

    use std::sync::mpsc::sync_channel;

    fn get_colour_range() -> ColourRange {
        ColourRange::new(vec![
            ColourBucket::new("blue", 14.0, Colour(10, 10, 226)),
            ColourBucket::new("orange", 18.0, Colour(120, 20, 0)),
            ColourBucket::new("salmon", 22.0, Colour(160, 10, 1)),
            ColourBucket::new("coral", 26.0, Colour(255, 1, 1)),
            ColourBucket::new("red", 30.0, Colour(255, 0, 100)),
        ])
        .unwrap()
    }

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

    fn new_measurement_event(temperature: f64, humidity: f64) -> Event {
        Event::new(Message::Environment(Measurement {
            temperature,
            humidity,
        }))
    }

    struct MockLEDs {
        called: bool,
        last_colours: Vec<Colour>,
        last_brightness: f32,
    }

    impl MockLEDs {
        fn new() -> MockLEDs {
            MockLEDs {
                called: false,
                last_colours: vec![],
                last_brightness: 0.0,
            }
        }
    }

    impl LEDs for &mut MockLEDs {
        fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String> {
            self.called = true;
            self.last_colours = Vec::from(colours);
            self.last_brightness = brightness;
            Ok(())
        }
    }

    #[test]
    fn main_loop_does_not_set_leds_if_no_events_are_received() {
        // arrange
        let (sender, receiver) = sync_channel(10);
        let mut leds = MockLEDs::new();
        let colour_range = get_colour_range();

        // act
        drop(sender);
        main_loop(receiver, &mut leds, colour_range).unwrap();

        // assert
        assert!(!leds.called);
    }

    #[test]
    fn main_loop_sets_leds_correctly_on_new_measurement_data() {
        // arrange
        let (sender, receiver) = sync_channel(10);
        let mut leds = MockLEDs::new();
        let colour_range = get_colour_range();

        // act
        sender.send(new_measurement_event(12.0, 13.0)).unwrap();
        drop(sender);
        main_loop(receiver, &mut leds, colour_range).unwrap();

        // assert
        assert!(leds.called);
        assert_eq!(leds.last_colours, vec![Colour(10, 10, 226); 8]);
        assert_eq!(leds.last_brightness, 0.05);
    }

    #[test]
    fn main_loop_sets_leds_correctly_on_new_tap_events() {
        // arrange
        let (sender, receiver) = sync_channel(10);
        let mut leds = MockLEDs::new();
        let colour_range = get_colour_range();

        // act
        sender.send(Event::new(Message::TapEvent)).unwrap();
        drop(sender);
        main_loop(receiver, &mut leds, colour_range).unwrap();

        // assert
        assert!(leds.called);
        assert_eq!(leds.last_colours, vec![Colour(0, 0, 0); 8]);
        assert_eq!(leds.last_brightness, 0.5);
    }
}
