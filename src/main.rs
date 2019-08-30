extern crate glow;
extern crate rppal;

use glow::start_am2320;
use glow::{main_loop, BlinktLEDs, Bucket, Colour, ColourRange};

fn main() -> Result<(), String> {
    let receiver = environment::start_am2320(30);
    let mut leds = BlinktLEDs::new();

    let colour_range = ColourRange::new(vec![
        Bucket::new("blue", 14.0, Colour(10, 10, 226)),
        Bucket::new("orange", 18.0, Colour(120, 20, 0)),
        Bucket::new("salmon", 22.0, Colour(160, 10, 1)),
        Bucket::new("coral", 26.0, Colour(255, 1, 1)),
        Bucket::new("red", 30.0, Colour(255, 0, 100)),
    ])?;

    main_loop(receiver, &mut leds, colour_range)
}
