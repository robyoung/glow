extern crate env_logger;
extern crate glow;
extern crate rppal;
#[macro_use]
extern crate log;

use std::env;

use glow::events::{run_loop, EventHandler};
use glow::leds::{
    BlinktLEDs, ColourRange, DynamicLEDBrightness, COLOUR_BLUE, COLOUR_CORAL, COLOUR_ORANGE,
    COLOUR_RED, COLOUR_SALMON,
};
use glow::{EnvironmentSensor, VibrationSensor};
use glow::{LEDHandler, WebHookHandler};

fn main() -> Result<(), String> {
    env_logger::init();

    let colour_range = ColourRange::new(
        14.0,
        4.0,
        &[
            COLOUR_BLUE,
            COLOUR_ORANGE,
            COLOUR_SALMON,
            COLOUR_CORAL,
            COLOUR_RED,
        ],
    )?;
    let leds = BlinktLEDs::new();
    let brightness = DynamicLEDBrightness::new(String::from(
        "https://raw.githubusercontent.com/robyoung/data/master/glow-brightness",
    ));
    let led_handler = LEDHandler::new_with_brightness(leds, colour_range, brightness);

    let mut handlers: Vec<Box<dyn EventHandler>> = vec![
        Box::new(EnvironmentSensor {}),
        Box::new(VibrationSensor {}),
        Box::new(led_handler),
    ];

    if let Ok(ifttt_webhook_key) = env::var("IFTTT_WEBHOOK_KEY") {
        debug!("Adding IFTTT web hook handler");
        let webhook_base_url =
            env::var("IFTT_WEBHOOK_URL").unwrap_or("https://maker.ifttt.com".to_string());

        let webhook_url = format!(
            "{}/trigger/glow-data/with/key/{}",
            webhook_base_url, ifttt_webhook_key
        );
        handlers.push(Box::new(WebHookHandler::new(webhook_url)));
    }

    run_loop(handlers);
    Ok(())
}