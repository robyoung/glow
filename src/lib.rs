extern crate am2320;
extern crate blinkt;
extern crate chrono;

use embedded_hal::blocking::{delay, i2c};
use std::{thread, time};

use am2320::AM2320;
use blinkt::Blinkt;
use chrono::{offset::Utc, DateTime};

static NUM_PIXELS: u8 = 8;
static ERROR_LIMIT: u8 = 3;
static LED_BRIGHTNESS: f32 = 0.05;

#[derive(Debug, Clone, PartialEq)]
pub struct Colour(pub u8, pub u8, pub u8);

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
}

pub trait Sensor {
    fn read(&mut self) -> Result<Vec<f32>, String>;
}

pub struct AM2320Sensor<I2C, Delay> {
    am2320: AM2320<I2C, Delay>,
}

impl<I2C, Delay, E> AM2320Sensor<I2C, Delay>
where
    I2C: i2c::Read<Error = E> + i2c::Write<Error = E>,
    Delay: delay::DelayUs<u16>,
{
    pub fn new(device: I2C, delay: Delay) -> Self {
        Self {
            am2320: AM2320::new(device, delay),
        }
    }
}

impl<I2C, Delay, E> Sensor for AM2320Sensor<I2C, Delay>
where
    I2C: i2c::Read<Error = E> + i2c::Write<Error = E>,
    Delay: delay::DelayUs<u16>,
{
    fn read(&mut self) -> Result<Vec<f32>, String> {
        match self.am2320.read() {
            Ok(measurement) => Ok(vec![
                measurement.temperature as f32,
                measurement.humidity as f32,
            ]),
            Err(err) => Err(format!("failed to read: {:?}", err)),
        }
    }
}

pub trait LEDs {
    fn show(&mut self, colours: Vec<Colour>, brightness: f32) -> Result<(), String>;
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
    fn show(&mut self, colours: Vec<Colour>, brightness: f32) -> Result<(), String> {
        for (pixel, colour) in colours.iter().enumerate() {
            self.blinkt
                .set_pixel_rgbb(pixel, colour.0, colour.1, colour.2, brightness);
        }

        if let Err(err) = self.blinkt.show() {
            Err(format!("{:?}", err))
        } else {
            Ok(())
        }
    }
}

fn data_is_roughly_equal(previous_data: &[f32], new_data: &[f32]) -> bool {
    previous_data
        .iter()
        .zip(new_data.iter())
        .all(|(&previous, &new)| {
            // TODO: pull out constant
            (previous - new).abs() < 0.001
        })
}

pub fn sync_loop(
    loop_sleep: u64,
    mut sensor: impl Sensor,
    mut leds: impl LEDs,
    colour_range: ColourRange,
) -> Result<(), String> {
    let mut error_count = 0;
    let mut previous_data: Option<Vec<f32>> = None;

    loop {
        let now: DateTime<Utc> = Utc::now();

        match sensor.read() {
            Err(err) => {
                error_count += 1;
                eprintln!("Error reading sensor {} times: {:?}", error_count, err);
                if error_count > ERROR_LIMIT {
                    return Err("Too many errors".to_string());
                }
            }
            Ok(new_data) => {
                error_count = 0;

                if let Some(previous_data) = &previous_data {
                    if data_is_roughly_equal(previous_data, &new_data) {
                        eprintln!("Skipping unchanged data");
                        continue;
                    }
                }

                previous_data = Some(new_data.clone());

                println!("{},data,{},{}", now.to_rfc3339(), new_data[0], new_data[1]);

                let pixels = colour_range.get_pixels(new_data[0]);

                leds.show(pixels, LED_BRIGHTNESS)?;
            }
        }

        thread::sleep(time::Duration::from_secs(loop_sleep));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSensor {
        iteration: u8,
    }

    impl MockSensor {
        fn new() -> MockSensor {
            MockSensor { iteration: 0 }
        }
    }

    impl Sensor for MockSensor {
        fn read(&mut self) -> Result<Vec<f32>, String> {
            let result = match self.iteration {
                0 => Ok(vec![23.4, 64.2]),
                1 => Ok(vec![28.2, 64.4]),
                _ => Err("Cannot read sensor".to_string()),
            };

            self.iteration += 1;

            result
        }
    }

    struct MockLEDs {
        pixels: Vec<Vec<Colour>>,
    }

    impl MockLEDs {
        fn new() -> MockLEDs {
            MockLEDs { pixels: vec![] }
        }
    }

    impl LEDs for &mut MockLEDs {
        fn show(&mut self, colours: Vec<Colour>, _brightness: f32) -> Result<(), String> {
            self.pixels.push(colours);
            Ok(())
        }
    }

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
    fn test_sync_loop() {
        // arrange
        let sensor = MockSensor::new();
        let mut leds = MockLEDs::new();
        let colour_range = get_colour_range();

        // act
        let result = sync_loop(0, sensor, &mut leds, colour_range);

        // assert
        assert!(result.is_err());
        assert_eq!(
            leds.pixels[0],
            vec![
                Colour(160, 10, 1),
                Colour(160, 10, 1),
                Colour(160, 10, 1),
                Colour(160, 10, 1),
                Colour(160, 10, 1),
                Colour(255, 1, 1),
                Colour(255, 1, 1),
                Colour(255, 1, 1)
            ]
        );
        assert_eq!(
            leds.pixels[1],
            vec![
                Colour(255, 1, 1),
                Colour(255, 1, 1),
                Colour(255, 1, 1),
                Colour(255, 1, 1),
                Colour(255, 0, 100),
                Colour(255, 0, 100),
                Colour(255, 0, 100),
                Colour(255, 0, 100)
            ]
        );
    }

    #[test]
    fn data_is_roughly_equal_when_within_limits() {
        // arrange
        let previous_data = vec![12.3001, 13.4001];
        let new_data = vec![12.3002, 13.4001];

        // assert
        assert!(data_is_roughly_equal(&previous_data, &new_data));
    }

    #[test]
    fn data_is_not_roughly_equal_when_outside_limits() {
        // arrange
        let previous_data = vec![12.3001, 13.4001];
        let new_data = vec![12.4012, 13.4001];

        // assert
        assert!(!data_is_roughly_equal(&previous_data, &new_data));
    }

}
