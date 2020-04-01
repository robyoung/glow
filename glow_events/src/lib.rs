use serde::{Deserialize, Serialize};

pub mod v1;
pub mod v2;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct Measurement {
    pub temperature: f64,
    pub humidity: f64,
}

impl Measurement {
    pub fn new(temperature: f64, humidity: f64) -> Self {
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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct TPLinkDevice {
    pub name: String,
}
