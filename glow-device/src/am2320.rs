//! Environment sensor
//!
//! TODO: investigate turning thread part into request / response service
use std::{sync::mpsc::sync_channel, thread};

use am2320::Am2320;
use log::{debug, error, info};
use rppal::{hal::Delay, i2c::I2c};
use tokio::time::{delay_for, Duration};

use glow_events::{
    v2::{Event, Message},
    Measurement,
};

use crate::events::Sender;
use core::time;

const SENSOR_ERROR_LIMIT: u8 = 3;
const SENSOR_ERROR_BACKOFF_LIMIT: u64 = 3;
const SENSOR_SLEEP: u64 = 30;
const SENSOR_MAX_SKIP: u8 = 10;

type ResponseSender = tokio::sync::oneshot::Sender<Measurement>;
type RequestReceiver = std::sync::mpsc::Receiver<ResponseSender>;

pub async fn handler(tx: Sender) {
    let (req_sender, req_receiver) = sync_channel(0);

    let mut previous_data: Option<Measurement> = None;
    let mut num_skipped: u8 = 0;

    thread::spawn(move || {
        run_worker(req_receiver);
    });

    loop {
        let (resp_sender, resp_receiver) = tokio::sync::oneshot::channel();
        req_sender
            .try_send(resp_sender)
            .expect("Could not request sensor reading");
        let measurement = resp_receiver.await.unwrap();

        if should_send(&measurement, &previous_data, num_skipped) {
            num_skipped = 0;
            debug!(
                "Sending changed data: {:?} {:?}",
                measurement, previous_data
            );
            previous_data = Some(measurement);

            let message = Message::new_event(Event::Measurement(measurement));
            tx.send(message)
                .expect("Failed to write sensor data to channel");
        } else {
            num_skipped += 1;
            debug!(
                "Skipping unchanged data: {:?} {:?}",
                measurement, previous_data
            );
        }

        let sleep = SENSOR_SLEEP + (SENSOR_SLEEP as f64 * 0.5 * num_skipped as f64) as u64;
        delay_for(Duration::from_secs(sleep)).await;
    }
}

fn run_worker(requests: RequestReceiver) {
    let mut sensor = Am2320::new(I2c::new().expect("could not initialise I2C"), Delay::new());

    // receive a request
    for sender in requests.iter() {
        sender
            // read the measurement and send the response
            .send(read_measurement(&mut sensor))
            .expect("failed to send environment sensor measurement");
    }
}

fn should_send(
    measurement: &Measurement,
    previous_data: &Option<Measurement>,
    num_skipped: u8,
) -> bool {
    let is_changed = if let Some(previous_data) = previous_data {
        !previous_data.temperature_roughly_equal(measurement)
    } else {
        true
    };
    is_changed || num_skipped > SENSOR_MAX_SKIP
}

fn read_measurement(sensor: &mut Am2320<I2c, Delay>) -> Measurement {
    let mut error_count: u8 = 0;
    let mut backoff_count: u64 = 0;
    loop {
        match sensor.read() {
            Ok(m) => {
                if error_count > 0 {
                    info!(
                        "AM2320 read success after {} failures: {:?} ",
                        error_count, m
                    );
                }
                return m.into();
            }
            Err(err) => {
                error!("AM232O read error: {:?}", err);
                error_count += 1;
                if error_count > SENSOR_ERROR_LIMIT {
                    let sleep = SENSOR_SLEEP * (backoff_count + 1);
                    error!("too many errors, backing off for {}s", sleep);
                    thread::sleep(time::Duration::from_secs(sleep));
                    error_count = 0;
                    if backoff_count < SENSOR_ERROR_BACKOFF_LIMIT {
                        backoff_count += 1;
                    } else {
                        error!("environment sensor backoff limit reached; shutting down");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // TODO move these tests to where the impl is

    #[test]
    fn data_is_roughly_equal_when_within_limits() {
        // arrange
        let previous_data = Measurement {
            temperature: 12.3001,
            humidity: 13.4001,
        };
        let new_data = Measurement {
            temperature: 12.3002,
            humidity: 13.4001,
        };

        // assert
        assert!((&previous_data).roughly_equal(&new_data));
    }

    #[test]
    fn data_is_not_roughly_equal_when_outside_limits() {
        // arrange
        let previous_data = Measurement {
            temperature: 12.3001,
            humidity: 13.4001,
        };
        let new_data = Measurement {
            temperature: 12.4012,
            humidity: 13.4001,
        };

        // assert
        assert!(!(&previous_data).roughly_equal(&new_data));
    }
}
