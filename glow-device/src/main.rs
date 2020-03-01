extern crate env_logger;
extern crate glow_device;
extern crate rppal;
#[macro_use]
extern crate log;

use std::env;

use glow_device::events::{run_loop, EventHandler};
use glow_device::leds::{
    BlinktLEDs, ColourRange, COLOUR_BLUE, COLOUR_CORAL, COLOUR_ORANGE, COLOUR_RED, COLOUR_SALMON,
};
use glow_device::{EnvironmentSensor, VibrationSensor};
use glow_device::{LEDBrightnessHandler, LEDHandler, WebHookHandler, WebEventHandler};

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
    let brightness_handler = LEDBrightnessHandler::new(
        &"https://raw.githubusercontent.com/robyoung/data/master/glow-brightness",
    );
    let led_handler = LEDHandler::new(leds, colour_range);

    let mut handlers: Vec<Box<dyn EventHandler>> = vec![
        Box::new(EnvironmentSensor {}),
        Box::new(VibrationSensor {}),
        Box::new(brightness_handler),
        Box::new(led_handler),
    ];

    if let Ok(ifttt_webhook_key) = env::var("IFTTT_WEBHOOK_KEY") {
        debug!("Adding IFTTT web hook handler");
        let webhook_base_url =
            env::var("IFTTT_WEBHOOK_URL").unwrap_or_else(|_| "https://maker.ifttt.com".to_string());

        let webhook_url = format!(
            "{}/trigger/glow-data/with/key/{}",
            webhook_base_url, ifttt_webhook_key
        );
        handlers.push(Box::new(WebHookHandler::new(webhook_url)));
    }

    if let (Ok(web_event_url), Ok(web_event_token)) = (env::var("WEB_EVENT_URL"), env::var("WEB_EVENT_TOKEN")) {
        info!("Adding web event handler");
        handlers.push(Box::new(WebEventHandler::new(web_event_url, web_event_token)));
    }

    run_loop(handlers);
    Ok(())
}
