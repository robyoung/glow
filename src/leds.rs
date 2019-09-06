use blinkt::Blinkt;

static NUM_PIXELS: u8 = 8;

pub enum LedBrightness {
    Dim,
    Bright,
    Off,
}

impl LedBrightness {
    pub fn next(self) -> LedBrightness {
        match self {
            LedBrightness::Dim => LedBrightness::Bright,
            LedBrightness::Bright => LedBrightness::Off,
            LedBrightness::Off => LedBrightness::Dim,
        }
    }

    pub fn value(&self) -> f32 {
        match self {
            LedBrightness::Dim => 0.05,
            LedBrightness::Bright => 0.5,
            LedBrightness::Off => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Colour(pub u8, pub u8, pub u8);

impl Colour {
    pub fn black() -> Colour {
        Colour(0, 0, 0)
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

pub struct ColourRange {
    buckets: Vec<ColourBucket>,
    num_pixels: u8,
}

impl ColourRange {
    pub fn new(buckets: Vec<ColourBucket>) -> Result<ColourRange, String> {
        if buckets.is_empty() {
            Err("not long enough".to_string())
        } else {
            Ok(ColourRange {
                buckets,
                num_pixels: NUM_PIXELS,
            })
        }
    }

    // TODO: sort buckets?

    pub fn get_pixels(&self, value: f32) -> Vec<Colour> {
        let first = self.buckets.first().unwrap();
        if value <= first.value {
            return vec![first.colour.clone(); self.num_pixels as usize];
        }

        let last = self.buckets.last().unwrap();
        if value >= last.value {
            return vec![last.colour.clone(); self.num_pixels as usize];
        }

        for i in 0..self.buckets.len() - 1 {
            let (bottom, top) = (&self.buckets[i], &self.buckets[i + 1]);
            if bottom.value <= value && value <= top.value {
                let bottom_to_value = value - bottom.value;
                let bottom_to_top = top.value - bottom.value;
                let num_pixels =
                    (f32::from(self.num_pixels) * (bottom_to_value / bottom_to_top)).round() as u8;

                let mut pixels =
                    vec![bottom.colour.clone(); (self.num_pixels - num_pixels) as usize];
                let top_pixels = vec![top.colour.clone(); num_pixels as usize];
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
    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String>;
}

pub struct BlinktLEDs {
    blinkt: Blinkt,
}

impl BlinktLEDs {
    pub fn new() -> Self {
        Self {
            blinkt: Blinkt::new().unwrap(),
        }
    }
}

impl Default for BlinktLEDs {
    fn default() -> Self {
        Self::new()
    }
}

impl LEDs for &mut BlinktLEDs {
    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String> {
        for (pixel, colour) in colours.iter().enumerate() {
            self.blinkt
                .set_pixel_rgbb(pixel, colour.0, colour.1, colour.2, brightness);
        }

        if let Err(err) = self.blinkt.show() {
            Err(format!("Failed to write LEDs: {:?}", err))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_create_colour_range_with_no_buckets() {
        // arrange
        let colour_range = ColourRange::new(vec![]);

        // assert
        assert!(colour_range.is_err());
    }

    fn get_colour_range() -> ColourRange {
        ColourRange::new(vec![
            ColourBucket::new("blue", 14.0, Colour(10, 10, 226)),
            ColourBucket::new("orange", 18.0, Colour(120, 20, 0)),
            ColourBucket::new("salmon", 22.0, Colour(160, 10, 1)),
            ColourBucket::new("coral", 26.0, Colour(255, 1, 1)),
            ColourBucket::new("red", 30.0, Colour(255, 0, 100)),
        ])
        .unwrap()
    }

    #[test]
    fn get_pixels_returns_all_pixels_as_colour_when_only_one_bucket() {
        // arrange
        let colour_range =
            ColourRange::new(vec![ColourBucket::new("blue", 14.0, Colour(10, 10, 226))]).unwrap();

        // assert
        assert!(colour_range.get_pixels(12.0) == vec![Colour(10, 10, 226); 8]);
        assert!(colour_range.get_pixels(14.0) == vec![Colour(10, 10, 226); 8]);
        assert!(colour_range.get_pixels(18.0) == vec![Colour(10, 10, 226); 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_lower_bound() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert!(colour_range.get_pixels(12.0) == vec![Colour(10, 10, 226); 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_upper_bound() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert!(colour_range.get_pixels(31.0) == vec![Colour(255, 0, 100); 8]);
    }

    #[test]
    fn get_pixels_with_multiple_colour_ranges_split_pixels() {
        // arrange
        let colour_range = get_colour_range();

        // assert
        assert_eq!(
            colour_range.get_pixels(16.0),
            vec![
                Colour(10, 10, 226),
                Colour(10, 10, 226),
                Colour(10, 10, 226),
                Colour(10, 10, 226),
                Colour(120, 20, 0),
                Colour(120, 20, 0),
                Colour(120, 20, 0),
                Colour(120, 20, 0)
            ]
        );
    }
}