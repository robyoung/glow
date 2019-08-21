extern crate glow;
extern crate rppal;

use rppal::{hal::Delay, i2c::I2c};

use glow::{sync_loop, Bucket, Colour, ColourRange};
use glow::{AM2320Sensor, BlinktLEDs};

fn main() -> Result<(), String> {
    let device = I2c::new().expect("could not initialise I2C");
    let delay = Delay::new();

    let sensor = AM2320Sensor::new(device, delay);
    let mut leds = BlinktLEDs::new();

    let colour_range = ColourRange::new(vec![
        Bucket::new("blue", 14.0, Colour(10, 10, 226)),
        Bucket::new("orange", 18.0, Colour(120, 20, 0)),
        Bucket::new("salmon", 22.0, Colour(160, 10, 1)),
        Bucket::new("coral", 26.0, Colour(255, 1, 1)),
        Bucket::new("red", 30.0, Colour(255, 0, 100)),
    ])?;

    sync_loop(30, sensor, &mut leds, colour_range)
}
