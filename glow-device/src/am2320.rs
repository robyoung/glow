//! Environment sensor
//!
//! TODO: investigate turning thread part into request / response service
use std::{sync::mpsc::sync_channel, thread};

use am2320::Am2320;
use log::{debug, error, info};
use rppal::{hal::Delay, i2c::I2c};
use tokio::time::{delay_for, Duration};

use glow_events::{
    v2::{Command, Event, Message},
    Measurement,
};

use crate::events::Sender;
use core::time;

const SENSOR_ERROR_LIMIT: u8 = 3;
const SENSOR_ERROR_BACKOFF_LIMIT: u64 = 3;
const SENSOR_SLEEP: u64 = 30;
const SENSOR_MAX_SKIP: u8 = 10;

type ResponseSender = tokio::sync::oneshot::Sender<Option<Measurement>>;
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

        if let Some(message) = handle_measurement(measurement, &mut previous_data, &mut num_skipped)
        {
            tx.send(message)
                .expect("Failed to write sensor data to channel");
        }

        let sleep = SENSOR_SLEEP + (SENSOR_SLEEP as f64 * 0.5 * num_skipped as f64) as u64;
        delay_for(Duration::from_secs(sleep)).await;
    }
}

fn handle_measurement(
    measurement: Option<Measurement>,
    previous_data: &mut Option<Measurement>,
    num_skipped: &mut u8,
) -> Option<Message> {
    if let Some(measurement) = measurement {
        if should_send(&measurement, previous_data, *num_skipped) {
            *num_skipped = 0;
            debug!(
                "Sending changed data: {:?} {:?}",
                measurement, previous_data
            );
            *previous_data = Some(measurement);

            Some(Message::new_event(Event::Measurement(measurement)))
        } else {
            *num_skipped += 1;
            debug!(
                "Skipping unchanged data: {:?} {:?}",
                measurement, previous_data
            );
            None
        }
    } else {
        Some(Message::new_command(Command::Stop))
    }
}

fn run_worker(requests: RequestReceiver) {
    let mut sensor = Am2320::new(I2c::new().expect("could not initialise I2C"), Delay::new());

    // receive a request
    for sender in requests.iter() {
        sender
            // read the measurement and send the response
            .send(read_measurement(&mut sensor, SENSOR_SLEEP))
            .expect("failed to send environment sensor measurement");
    }
}

type SensorResult = Result<am2320::Measurement, am2320::Error>;

trait Sensor {
    fn read(&mut self) -> SensorResult;
}

impl Sensor for Am2320<I2c, Delay> {
    fn read(&mut self) -> SensorResult {
        self.read()
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

fn read_measurement<S: Sensor>(sensor: &mut S, sensor_sleep: u64) -> Option<Measurement> {
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
                return Some(m.into());
            }
            Err(err) => {
                error!("AM232O read error: {:?}", err);
                error_count += 1;
                if error_count > SENSOR_ERROR_LIMIT {
                    let sleep = sensor_sleep * (backoff_count + 1);
                    error!("too many errors, backing off for {}s", sleep);
                    thread::sleep(time::Duration::from_secs(sleep));
                    error_count = 0;
                    if backoff_count < SENSOR_ERROR_BACKOFF_LIMIT {
                        backoff_count += 1;
                    } else {
                        error!("environment sensor backoff limit reached; shutting down");
                        return None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSensor {
        values: Vec<SensorResult>,
    }

    impl MockSensor {
        fn new(values: Vec<SensorResult>) -> Self {
            Self { values }
        }
    }

    impl Sensor for MockSensor {
        fn read(&mut self) -> SensorResult {
            self.values.remove(0)
        }
    }

    const AM2320_MEASUREMENT: am2320::Measurement = am2320::Measurement {
        temperature: 1.1,
        humidity: 2.2,
    };
    const MEASUREMENT: Measurement = Measurement::new(1.1, 2.2);

    #[test]
    fn read_a_measurement() {
        let mut sensor = MockSensor::new(vec![Ok(AM2320_MEASUREMENT)]);
        let read_measurement = read_measurement(&mut sensor, 0).unwrap();

        assert_eq!(read_measurement, Measurement::from(AM2320_MEASUREMENT));
    }

    #[test]
    fn read_a_measurement_after_one_failure() {
        let mut sensor =
            MockSensor::new(vec![Err(am2320::Error::WriteError), Ok(AM2320_MEASUREMENT)]);
        let read_measurement = read_measurement(&mut sensor, 0).unwrap();

        assert_eq!(read_measurement, Measurement::from(AM2320_MEASUREMENT));
    }

    #[test]
    fn read_a_measurement_after_four_failures() {
        let mut sensor = MockSensor::new(vec![
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Ok(AM2320_MEASUREMENT),
        ]);
        let read_measurement = read_measurement(&mut sensor, 0).unwrap();

        assert_eq!(read_measurement, Measurement::from(AM2320_MEASUREMENT));
    }

    #[test]
    fn read_a_measurement_until_backoff_exceeded() {
        let mut sensor = MockSensor::new(vec![
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Err(am2320::Error::WriteError),
            Ok(AM2320_MEASUREMENT),
        ]);
        let read_measurement = read_measurement(&mut sensor, 0);

        assert!(read_measurement.is_none());
    }

    #[test]
    fn handle_measurement_stop() {
        // arrange
        let mut previous_data = None;
        let mut num_skipped = 0;

        // act
        let message = handle_measurement(None, &mut previous_data, &mut num_skipped).unwrap();

        // assert
        assert_eq!(message.into_command(), Some(Command::Stop));
    }

    #[test]
    fn handle_measurement_some() {
        // arrange
        let mut previous_data = None;
        let mut num_skipped = 0;

        // act
        let message =
            handle_measurement(Some(MEASUREMENT), &mut previous_data, &mut num_skipped).unwrap();

        // assert
        assert_eq!(message.into_event(), Some(Event::Measurement(MEASUREMENT)));
    }

    #[test]
    fn handle_measurement_skip() {
        // arrange
        let mut previous_data = Some(MEASUREMENT);
        let mut num_skipped = 0;

        // act
        let message = handle_measurement(Some(MEASUREMENT), &mut previous_data, &mut num_skipped);

        // assert
        assert!(message.is_none());
    }
}
