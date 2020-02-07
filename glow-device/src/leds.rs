use std::{cell::Cell, cmp::Ordering, f32, fmt, thread, time};

use blinkt::Blinkt;

const NUM_PIXELS: u8 = 8;

pub const COLOUR_BLUE: Colour = Colour(10, 10, 226);
pub const COLOUR_ORANGE: Colour = Colour(120, 20, 0);
pub const COLOUR_SALMON: Colour = Colour(160, 10, 1);
pub const COLOUR_CORAL: Colour = Colour(255, 1, 1);
pub const COLOUR_RED: Colour = Colour(255, 0, 100);

pub trait LedBrightness {
    fn next(&mut self);
    fn value(&self) -> f32;
}

pub enum StaticLedBrightness {
    Dim,
    Bright,
    Off,
}

impl LedBrightness for StaticLedBrightness {
    fn next(&mut self) {
        *self = match self {
            StaticLedBrightness::Dim => StaticLedBrightness::Bright,
            StaticLedBrightness::Bright => StaticLedBrightness::Off,
            StaticLedBrightness::Off => StaticLedBrightness::Dim,
        };
    }

    fn value(&self) -> f32 {
        match self {
            StaticLedBrightness::Dim => 0.05,
            StaticLedBrightness::Bright => 0.5,
            StaticLedBrightness::Off => 0.0,
        }
    }
}

pub struct DynamicLEDBrightness {
    client: ureq::Agent,
    url: String,
    last_received: time::Instant,
    current_value: Cell<Option<f32>>,
}

const LED_BRIGHTNESS_UPDATE_TIME: u64 = 2;

impl DynamicLEDBrightness {
    pub fn new(url: String) -> DynamicLEDBrightness {
        DynamicLEDBrightness {
            client: ureq::agent(),
            url,
            last_received: time::Instant::now() - time::Duration::from_secs(100_000),
            current_value: Cell::new(None),
        }
    }

    fn get_value(&self) -> Option<f32> {
        let resp = self.client.get(self.url.as_str()).call();
        if resp.error() {
            error!(
                "Failed to get new LED brightness value: {:?}",
                resp.status()
            );
            return None;
        }

        if let Ok(data) = resp.into_string() {
            if let Ok(value) = data.trim().parse::<f32>() {
                return Some(value);
            } else {
                error!("Failed to parse LED brightness value: {}", data);
            }
        } else {
            error!("Failed to read LED brightness response");
        }
        None
    }
}

impl LedBrightness for DynamicLEDBrightness {
    fn next(&mut self) {}
    fn value(&self) -> f32 {
        if self.current_value.get().is_none()
            || self.last_received.elapsed() > time::Duration::from_secs(LED_BRIGHTNESS_UPDATE_TIME)
        {
            debug!("Updating LED brightness");
            if let Some(value) = self.get_value() {
                self.current_value.set(Some(value));
            }
        }
        self.current_value.get().unwrap()
    }
}

#[derive(Clone, PartialEq, Eq, Copy)]
pub struct Colour(pub u8, pub u8, pub u8);

impl Colour {
    pub fn black() -> Colour {
        Colour(0, 0, 0)
    }

    pub fn red() -> Colour {
        Colour(255, 0, 0)
    }

    pub fn green() -> Colour {
        Colour(0, 255, 0)
    }

    pub fn blue() -> Colour {
        Colour(10, 10, 226)
    }

    pub fn name(self) -> &'static str {
        match self {
            COLOUR_BLUE => "blue",
            COLOUR_ORANGE => "orange",
            COLOUR_SALMON => "salmon",
            COLOUR_CORAL => "coral",
            COLOUR_RED => "red",
            _ => "unnamed",
        }
    }
}

impl fmt::Debug for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Colour[{}]({}, {}, {})",
            self.name(),
            self.0,
            self.1,
            self.2
        )
    }
}

pub struct ColourBucket {
    name: String,
    value: f32,
    colour: Colour,
}

impl ColourBucket {
    pub fn new(name: &str, value: f32, colour: Colour) -> ColourBucket {
        ColourBucket {
            name: name.to_string(),
            value,
            colour,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn value(&self) -> &f32 {
        &self.value
    }
}

impl Ord for ColourBucket {
    fn cmp(&self, other: &ColourBucket) -> Ordering {
        if self.value < other.value {
            Ordering::Less
        } else if self.value > other.value {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl PartialOrd for ColourBucket {
    fn partial_cmp(&self, other: &ColourBucket) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ColourBucket {
    fn eq(&self, other: &ColourBucket) -> bool {
        self.name == other.name && self.value == other.value && self.colour == other.colour
    }
}

impl Eq for ColourBucket {}

pub struct ColourRange {
    buckets: Vec<ColourBucket>,
    num_pixels: u8,
}

impl ColourRange {
    pub fn new(lower: f32, step: f32, colours: &[Colour]) -> Result<ColourRange, String> {
        if colours.is_empty() {
            Err("must have at least one colour".to_string())
        } else {
            let buckets = colours
                .iter()
                .enumerate()
                .map(|(i, &colour)| {
                    ColourBucket::new(colour.name(), lower + (i as f32) * step, colour)
                })
                .collect();
            Ok(ColourRange {
                buckets,
                num_pixels: NUM_PIXELS,
            })
        }
    }

    pub fn get_pixels(&self, value: f32) -> Vec<Colour> {
        let first = self.buckets.first().unwrap();
        if value <= first.value {
            return vec![first.colour; self.num_pixels as usize];
        }

        let last = self.buckets.last().unwrap();
        if value >= last.value {
            return vec![last.colour; self.num_pixels as usize];
        }

        for i in 0..self.buckets.len() - 1 {
            let (bottom, top) = (&self.buckets[i], &self.buckets[i + 1]);
            if bottom.value <= value && value <= top.value {
                let bottom_to_value = value - bottom.value;
                let bottom_to_top = top.value - bottom.value;
                let num_pixels =
                    (f32::from(self.num_pixels) * (bottom_to_value / bottom_to_top)).round() as u8;

                let mut pixels = vec![bottom.colour; (self.num_pixels - num_pixels) as usize];
                let top_pixels = vec![top.colour; num_pixels as usize];
                pixels.extend(top_pixels);
                return pixels;
            }
        }
        unreachable!();
    }

    pub fn all(&self, colour: Colour) -> Vec<Colour> {
        vec![colour; self.num_pixels as usize]
    }
}

pub trait LEDs {
    fn party(&mut self) -> Result<(), String> {
        let colours = [Colour::red(), Colour::green(), Colour::blue()];
        let mut current_colours = [Colour::black(); NUM_PIXELS as usize];

        for colour in colours.iter() {
            for i in 0..NUM_PIXELS {
                current_colours[i as usize] = *colour;
                self.show(&current_colours, StaticLedBrightness::Bright.value())?;
                thread::sleep(time::Duration::from_millis(50));
            }
        }
        Ok(())
    }

    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String>;
}

pub struct BlinktLEDs {
    blinkt: Blinkt,
    current: Option<(Vec<Colour>, f32)>,
}

impl BlinktLEDs {
    pub fn new() -> Self {
        Self {
            blinkt: Blinkt::new().unwrap(),
            current: None,
        }
    }

    #[allow(clippy::if_same_then_else)]
    fn should_update(&mut self, colours: &[Colour], brightness: f32) -> bool {
        let result = match &self.current {
            None => true,
            Some(current) => {
                if (current.1 - brightness).abs() < f32::EPSILON {
                    true
                } else {
                    colours.iter().zip(current.0.iter()).any(|(&a, &b)| a != b)
                }
            }
        };
        if result {
            self.current = Some((colours.to_vec(), brightness));
        }

        result
    }
}

/// calculate brightness to send to Blinkt
///
/// The Blinkt will switch a LED off with a brightness of less than 0.04.
/// However, we can reduce the overall brightness by reducing the number of
/// LEDs that are switched on. There are 8 LEDs on the Blinkt the illumination
/// pattern below 0.04 will be as follows.
///
/// 0.01  *      *
/// 0.02  *  **  *
/// 0.03  * ** ***
/// 0.04  ********
#[allow(clippy::if_same_then_else)]
pub(self) fn get_blinkt_brightness(pixel: usize, brightness: f32) -> f32 {
    if [1, 2, 3, 4, 5, 6].contains(&pixel) && (brightness - 0.01).abs() < f32::EPSILON {
        0.0
    } else if [1, 2, 5, 6].contains(&pixel) && (brightness - 0.02).abs() < f32::EPSILON {
        0.0
    } else if [1, 4].contains(&pixel) && (brightness - 0.03).abs() < f32::EPSILON {
        0.0
    } else if brightness < 0.01 {
        0.0
    } else if brightness < 0.04 {
        0.04
    } else {
        brightness
    }
}

impl Default for BlinktLEDs {
    fn default() -> Self {
        Self::new()
    }
}

impl LEDs for BlinktLEDs {
    // TODO: maybe refactor so that Colour includes brightness
    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String> {
        if self.should_update(colours, brightness) {
            for (pixel, colour) in colours.iter().enumerate() {
                self.blinkt.set_pixel_rgbb(
                    pixel,
                    colour.0,
                    colour.1,
                    colour.2,
                    get_blinkt_brightness(pixel, brightness),
                );
            }

            if let Err(err) = self.blinkt.show() {
                return Err(format!("Failed to write LEDs: {:?}", err));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_create_colour_range_with_no_buckets() {
        // arrange
        let colour_range = ColourRange::new(0.0, 0.0, &[]);

        // assert
        assert!(colour_range.is_err());
    }

    fn get_colour_range() -> ColourRange {
        ColourRange::new(
            14.0,
            4.0,
            &[
                COLOUR_BLUE,
                COLOUR_ORANGE,
                COLOUR_SALMON,
                COLOUR_CORAL,
                COLOUR_RED,
            ],
        )
        .unwrap()
    }

    #[test]
    fn get_pixels_returns_all_pixels_as_colour_when_only_one_bucket() {
        // arrange
        let colour_range = ColourRange::new(14.0, 4.0, &[COLOUR_BLUE]).unwrap();

        // assert
        assert!(colour_range.get_pixels(12.0) == vec![COLOUR_BLUE; 8]);
        assert!(colour_range.get_pixels(14.0) == vec![COLOUR_BLUE; 8]);
        assert!(colour_range.get_pixels(18.0) == vec![COLOUR_BLUE; 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_lower_bound() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert!(colour_range.get_pixels(12.0) == vec![COLOUR_BLUE; 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_upper_bound() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert!(colour_range.get_pixels(31.0) == vec![COLOUR_RED; 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_split_pixels() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert_eq!(
            colour_range.get_pixels(16.0),
            vec![
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE
            ]
        );
        assert_eq!(
            colour_range.get_pixels(17.0),
            vec![
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
            ]
        );
    }

    fn test_blinkt_brightness_helper(brightness: f32, expected: [f32; 8]) {
        let actual = [brightness; 8]
            .iter()
            .enumerate()
            .map(|(pixel, brightness)| get_blinkt_brightness(pixel, *brightness))
            .collect::<Vec<f32>>();

        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn test_blinkt_brightness() {
        test_blinkt_brightness_helper(0.0, [0.0; 8]);
        test_blinkt_brightness_helper(0.005, [0.0; 8]);
        test_blinkt_brightness_helper(0.01, [0.04, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.04]);
        test_blinkt_brightness_helper(0.02, [0.04, 0.0, 0.0, 0.04, 0.04, 0.0, 0.0, 0.04]);
        test_blinkt_brightness_helper(0.03, [0.04, 0.0, 0.04, 0.04, 0.0, 0.04, 0.04, 0.04]);
        test_blinkt_brightness_helper(0.03, [0.04, 0.0, 0.04, 0.04, 0.0, 0.04, 0.04, 0.04]);
        test_blinkt_brightness_helper(0.04, [0.04; 8]);
        test_blinkt_brightness_helper(0.05, [0.05; 8]);
        test_blinkt_brightness_helper(0.1, [0.1; 8]);
    }
}
