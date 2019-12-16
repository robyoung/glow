extern crate env_logger;
extern crate glow;
extern crate rppal;
#[macro_use]
extern crate log;

use std::env;

use glow::events::{run_loop, EventHandler, EventSource};
use glow::leds::{BlinktLEDs, Colour, ColourBucket, ColourRange, DynamicLEDBrightness};
use glow::{EnvironmentSensor, VibrationSensor};
use glow::{LEDHandler, WebHookHandler};

fn main() -> Result<(), String> {
    env_logger::init();

    let colour_range = ColourRange::new(vec![
        ColourBucket::new("blue", 14.0, Colour(10, 10, 226)),
        ColourBucket::new("orange", 18.0, Colour(120, 20, 0)),
        ColourBucket::new("salmon", 22.0, Colour(160, 10, 1)),
        ColourBucket::new("coral", 26.0, Colour(255, 1, 1)),
        ColourBucket::new("red", 30.0, Colour(255, 0, 100)),
    ])?;
    let leds = BlinktLEDs::new();

    let sources: Vec<Box<dyn EventSource>> =
        vec![Box::new(EnvironmentSensor {}), Box::new(VibrationSensor {})];
    let brightness = DynamicLEDBrightness::new(String::from(
        "https://raw.githubusercontent.com/robyoung/data/master/glow-brightness",
    ));
    let mut handlers: Vec<Box<dyn EventHandler>> = vec![Box::new(LEDHandler::new_with_brightness(
        leds,
        colour_range,
        brightness,
    ))];

    if let Ok(ifttt_webhook_key) = env::var("IFTTT_WEBHOOK_KEY") {
        debug!("Adding IFTTT web hook handler");

        let webhook_url = format!(
            "https://maker.ifttt.com/trigger/glow-data/with/key/{}",
            ifttt_webhook_key
        );
        handlers.push(Box::new(WebHookHandler::new(webhook_url)));
    }

    run_loop(sources, handlers);
    Ok(())
}
