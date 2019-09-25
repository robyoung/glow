extern crate glow;
extern crate rppal;

use std::{env};

use glow::leds::{BlinktLEDs, Colour, ColourBucket, ColourRange};
use glow::events::{EventSource, EventHandler, run_loop};
use glow::{start_environment_sensor, start_vibration_sensor};
use glow::{PrintMeasurementHandler, LEDHandler, WebHookHandler};

fn main() -> Result<(), String> {
    let colour_range = ColourRange::new(vec![
        ColourBucket::new("blue", 14.0, Colour(10, 10, 226)),
        ColourBucket::new("orange", 18.0, Colour(120, 20, 0)),
        ColourBucket::new("salmon", 22.0, Colour(160, 10, 1)),
        ColourBucket::new("coral", 26.0, Colour(255, 1, 1)),
        ColourBucket::new("red", 30.0, Colour(255, 0, 100)),
    ])?;
    let leds = BlinktLEDs::new();

    let sources: Vec<EventSource> = vec![start_environment_sensor, start_vibration_sensor];
    let mut handlers: Vec<Box<dyn EventHandler>> = vec![
        Box::new(PrintMeasurementHandler {}),
        Box::new(LEDHandler::new(leds, colour_range)),
    ];

    if let Ok(ifttt_webhook_key) = env::var("IFTTT_WEBHOOK_KEY") {

        let webhook_url = format!(
            "https://maker.ifttt.com/trigger/glow-data/with/key/{}", ifttt_webhook_key
        );
        handlers.push(Box::new(WebHookHandler::new(webhook_url)));
    }

    run_loop(sources, handlers);
    Ok(())
}
