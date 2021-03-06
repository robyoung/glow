use serde::{Deserialize, Serialize};

pub mod v1;
pub mod v2;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct Measurement {
    pub temperature: f64,
    pub humidity: f64,
}

impl Measurement {
    pub const fn new(temperature: f64, humidity: f64) -> Self {
        Self {
            temperature,
            humidity,
        }
    }

    pub fn roughly_equal(&self, other: &Measurement) -> bool {
        self.temperature_roughly_equal(other) && (self.humidity - other.humidity).abs() < 0.001
    }

    pub fn temperature_roughly_equal(&self, other: &Measurement) -> bool {
        (self.temperature - other.temperature).abs() < 0.001
    }
}

impl From<am2320::Measurement> for Measurement {
    fn from(m: am2320::Measurement) -> Self {
        let temperature = m.temperature as f64;
        let humidity = m.humidity as f64;
        Self::new(
            (temperature * 100.0).round() / 100.0,
            (humidity * 100.0).round() / 100.0,
        )
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TPLinkDevice {
    pub name: String,
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
