extern crate am2320;
extern crate blinkt;
extern crate chrono;

use std::sync::mpsc::{Receiver, SyncSender};
use std::{thread, time};

use am2320::{Measurement, AM2320};

use blinkt::Blinkt;
use chrono::{offset::Utc, DateTime};
use rppal::{
    gpio::{Gpio, Trigger},
    hal::Delay,
    i2c::I2c,
};

static NUM_PIXELS: u8 = 8;
static ERROR_LIMIT: u8 = 3;
static INTERRUPT_PIN: u8 = 17;
static INTERRUPT_BOUNCE: u128 = 300;

enum LedBrightness {
    Dim,
    Bright,
    Off,
}

impl LedBrightness {
    fn next(self) -> LedBrightness {
        match self {
            LedBrightness::Dim => LedBrightness::Bright,
            LedBrightness::Bright => LedBrightness::Off,
            LedBrightness::Off => LedBrightness::Dim,
        }
    }

    fn value(&self) -> f32 {
        match self {
            LedBrightness::Dim => 0.05,
            LedBrightness::Bright => 0.5,
            LedBrightness::Off => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Colour(pub u8, pub u8, pub u8);

impl Colour {
    fn black() -> Colour {
        Colour(0, 0, 0)
    }
}

pub struct Bucket {
    name: String,
    value: f32,
    colour: Colour,
}

impl Bucket {
    pub fn new(name: &str, value: f32, colour: Colour) -> Bucket {
        Bucket {
            name: name.to_string(),
            value,
            colour,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn value(&self) -> &f32 {
        &self.value
    }
}

pub struct ColourRange {
    buckets: Vec<Bucket>,
    num_pixels: u8,
}

impl ColourRange {
    pub fn new(buckets: Vec<Bucket>) -> Result<ColourRange, String> {
        if buckets.is_empty() {
            Err("not long enough".to_string())
        } else {
            Ok(ColourRange {
                buckets,
                num_pixels: NUM_PIXELS,
            })
        }
    }

    // TODO: sort buckets?

    pub fn get_pixels(&self, value: f32) -> Vec<Colour> {
        let first = self.buckets.first().unwrap();
        if value <= first.value {
            return vec![first.colour.clone(); self.num_pixels as usize];
        }

        let last = self.buckets.last().unwrap();
        if value >= last.value {
            return vec![last.colour.clone(); self.num_pixels as usize];
        }

        for i in 0..self.buckets.len() - 1 {
            let (bottom, top) = (&self.buckets[i], &self.buckets[i + 1]);
            if bottom.value <= value && value <= top.value {
                let bottom_to_value = value - bottom.value;
                let bottom_to_top = top.value - bottom.value;
                let num_pixels =
                    (f32::from(self.num_pixels) * (bottom_to_value / bottom_to_top)).round() as u8;

                let mut pixels =
                    vec![bottom.colour.clone(); (self.num_pixels - num_pixels) as usize];
                let top_pixels = vec![top.colour.clone(); num_pixels as usize];
                pixels.extend(top_pixels);
                return pixels;
            }
        }
        unreachable!();
    }

    pub fn all(&self, colour: Colour) -> Vec<Colour> {
        vec![colour; self.num_pixels as usize]
    }
}

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

pub trait LEDs {
    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String>;
}

pub struct BlinktLEDs {
    blinkt: Blinkt,
}

impl BlinktLEDs {
    pub fn new() -> Self {
        Self {
            blinkt: Blinkt::new().unwrap(),
        }
    }
}

impl Default for BlinktLEDs {
    fn default() -> Self {
        Self::new()
    }
}

impl LEDs for &mut BlinktLEDs {
    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String> {
        for (pixel, colour) in colours.iter().enumerate() {
            self.blinkt
                .set_pixel_rgbb(pixel, colour.0, colour.1, colour.2, brightness);
        }

        if let Err(err) = self.blinkt.show() {
            Err(format!("Failed to write LEDs: {:?}", err))
        } else {
            Ok(())
        }
    }
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

    use std::sync::mpsc::sync_channel;

    #[test]
    fn cannot_create_colour_range_with_no_buckets() {
        // arrange
        let colour_range = ColourRange::new(vec![]);

        // assert
        assert!(colour_range.is_err());
    }

    fn get_colour_range() -> ColourRange {
        ColourRange::new(vec![
            Bucket::new("blue", 14.0, Colour(10, 10, 226)),
            Bucket::new("orange", 18.0, Colour(120, 20, 0)),
            Bucket::new("salmon", 22.0, Colour(160, 10, 1)),
            Bucket::new("coral", 26.0, Colour(255, 1, 1)),
            Bucket::new("red", 30.0, Colour(255, 0, 100)),
        ])
        .unwrap()
    }

    #[test]
    fn get_pixels_returns_all_pixels_as_colour_when_only_one_bucket() {
        // arrange
        let colour_range =
            ColourRange::new(vec![Bucket::new("blue", 14.0, Colour(10, 10, 226))]).unwrap();

        // assert
        assert!(colour_range.get_pixels(12.0) == vec![Colour(10, 10, 226); 8]);
        assert!(colour_range.get_pixels(14.0) == vec![Colour(10, 10, 226); 8]);
        assert!(colour_range.get_pixels(18.0) == vec![Colour(10, 10, 226); 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_lower_bound() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert!(colour_range.get_pixels(12.0) == vec![Colour(10, 10, 226); 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_upper_bound() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert!(colour_range.get_pixels(31.0) == vec![Colour(255, 0, 100); 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_split_pixels() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert_eq!(
            colour_range.get_pixels(16.0),
            vec![
                Colour(10, 10, 226),
                Colour(10, 10, 226),
                Colour(10, 10, 226),
                Colour(10, 10, 226),
                Colour(120, 20, 0),
                Colour(120, 20, 0),
                Colour(120, 20, 0),
                Colour(120, 20, 0)
            ]
        );
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
        assert_eq!(leds.last_brightness, 1.0);
    }
}
