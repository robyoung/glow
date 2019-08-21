extern crate glow;
extern crate rppal;

use rppal::{hal::Delay, i2c::I2c};

use glow::{ColourRange, Bucket, Colour};
use glow::{LEDs, BlinktLEDs};
use glow::{Sensor, AM2320Sensor};



fn read_mock_sensor() -> f32 {
    32.2
}


fn main() -> Result<(), String> {
    let device = I2c::new().expect("could not initialise I2C");
    let delay = Delay::new();

    let mut sensor = AM2320Sensor::new(device, delay);
    let mut leds = BlinktLEDs::new(); 

    let colour_range = ColourRange::new(
        vec![
            Bucket::new("blue", 14.0, Colour(10, 10, 226)),
            Bucket::new("orange", 18.0, Colour(120, 20, 0)),
            Bucket::new("salmon", 22.0, Colour(160, 10, 1)),
            Bucket::new("coral", 26.0, Colour(255, 1, 1)),
            Bucket::new("red", 30.0, Colour(255, 0, 100)),
        ],
    )?;
    // TODO: better name
    let value_flutter: f32 = 0.0001;
    
    let mut previous_value: Option<f32> = None;

    loop {
        let new_value = read_mock_sensor();

        if previous_value.is_some() && (new_value - previous_value.unwrap()).abs() < value_flutter {
            println!("skipping same value");
            continue;
        }

        let pixels = colour_range.get_pixels(new_value);

        println!("{:?}", pixels);

        previous_value = Some(new_value);
    }

}

