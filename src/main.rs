extern crate glow;
extern crate rppal;

use glow::leds::{BlinktLEDs, Colour, ColourBucket, ColourRange};
use glow::{main_loop, start_environment_sensor, start_vibration_sensor};
use std::sync::mpsc::sync_channel;

fn main() -> Result<(), String> {
    let (sender, receiver) = sync_channel(1);

    start_environment_sensor(sender.clone(), 30);
    start_vibration_sensor(sender.clone());

    let mut leds = BlinktLEDs::new();

    let colour_range = ColourRange::new(vec![
        ColourBucket::new("blue", 14.0, Colour(10, 10, 226)),
        ColourBucket::new("orange", 18.0, Colour(120, 20, 0)),
        ColourBucket::new("salmon", 22.0, Colour(160, 10, 1)),
        ColourBucket::new("coral", 26.0, Colour(255, 1, 1)),
        ColourBucket::new("red", 30.0, Colour(255, 0, 100)),
    ])?;

    main_loop(receiver, &mut leds, colour_range)
}
