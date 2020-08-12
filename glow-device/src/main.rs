use std::env;

use log::info;

use glow_device::events::Runner;

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut runner = Runner::default();
    runner.add(glow_device::tplink::handler);
    runner.add(glow_device::leds::handler);
    runner.add(glow_device::am2320::handler);
    runner.add(glow_device::vibration::handler);

    if let (Ok(web_event_url), Ok(web_event_token)) =
        (env::var("WEB_EVENT_URL"), env::var("WEB_EVENT_TOKEN"))
    {
        info!("Adding web event handler");
        runner.add(glow_device::web::WebHandler::new(
            web_event_url,
            web_event_token,
        ));
    }

    runner.run().await;
}
