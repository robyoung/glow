use std::{cmp::Ordering, convert::TryInto, f32, fmt, sync::mpsc::sync_channel, thread};

use blinkt::Blinkt;
use glow_events::v2::Message;
use log::{debug, error};
use tokio::time::{delay_for, Duration};

use crate::events::Sender;

const NUM_PIXELS: usize = 8;

pub const COLOUR_BLUE: Colour = Colour(10, 10, 100);
pub const COLOUR_ORANGE: Colour = Colour(120, 20, 0);
pub const COLOUR_SALMON: Colour = Colour(160, 10, 1);
pub const COLOUR_CORAL: Colour = Colour(255, 1, 1);
pub const COLOUR_RED: Colour = Colour(255, 0, 100);

pub async fn handler(tx: Sender) {
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
    )
    .unwrap();
    let mut colours = colour_range.all(Colour::black());
    let mut brightness = Brightness::default().value();
    let mut leds = BlinktBackgroundLEDs::new();
    let mut rx = tx.subscribe();

    use glow_events::v2::{Command::*, Event::*, Payload::*};
    while let Ok(message) = rx.recv().await {
        match message.payload() {
            Event(Measurement(measurement)) => {
                let new_colours = colour_range.get_pixels(measurement.temperature as f32);
                if new_colours.iter().zip(&colours).any(|(&a, &b)| a != b) {
                    colours = new_colours;
                    tx.send(Message::new_command(UpdateLEDs))
                        .expect("Failed to write TPLink device list to channel");
                } else {
                    debug!("Not updating unchanged LEDs");
                }
            }
            Event(SingleTap) => {
                brightness = Brightness::next_from(brightness).value();
                tx.send(Message::new_command(RunParty)).unwrap();
                tx.send(Message::new_command(UpdateLEDs)).unwrap();
            }
            Command(RunParty) => {
                // Have a party!
                //
                // Play a short flashing sequence on the LEDs
                // TODO: move this to a function?
                let colours = [Colour::red(), Colour::green(), Colour::blue()];
                let mut current_colours = [Colour::black(); NUM_PIXELS as usize];

                for colour in colours.iter() {
                    for i in 0..NUM_PIXELS {
                        current_colours[i as usize] = *colour;
                        leds.show(&current_colours, Brightness::Bright.value())
                            .await
                            .unwrap_or_else(|err| {
                                error!("party error: {}", err);
                            });
                        delay_for(Duration::from_millis(50)).await;
                    }
                }
            }
            Command(UpdateLEDs) => {
                if let Err(err) = leds.show(&colours, brightness).await {
                    error!("show error: {}", err);
                } else {
                    tx.send(Message::new_event(LEDColours(
                        colours.iter().map(|c| (c.0, c.1, c.2)).collect(),
                    )))
                    .unwrap();
                }
            }
            Command(SetBrightness(new_brightness)) => {
                brightness = *new_brightness;
                tx.send(Message::new_command(UpdateLEDs)).unwrap();
                tx.send(Message::new_event(LEDBrightness(*new_brightness)))
                    .unwrap();
            }
            _ => {}
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Brightness {
    Dim,
    Bright,
    Off,
}

impl Default for Brightness {
    fn default() -> Self {
        Self::Dim
    }
}

impl Brightness {
    /// Find the next brightness level from a given brightness
    ///
    /// Should go Off -> Dim -> Bright
    pub(crate) fn next_from(brightness: f32) -> Self {
        if brightness < Brightness::Dim.value() {
            Brightness::Dim
        } else if brightness < Brightness::Bright.value() {
            Brightness::Bright
        } else {
            Brightness::Off
        }
    }

    pub(crate) fn value(&self) -> f32 {
        match self {
            Brightness::Dim => 0.01,
            Brightness::Bright => 0.5,
            Brightness::Off => 0.0,
        }
    }
}

#[derive(Default, Clone, PartialEq, Eq, Copy)]
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

/// A colour and a value
///
/// Used to build a ColourRange. The value is the upper bound for this bucket.
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

/// A linear range of colours
///
/// Given a lower bound, a step and a set of colours we can map any value to our LED array.
pub struct ColourRange {
    buckets: Vec<ColourBucket>,
    num_pixels: u8,
}

impl ColourRange {
    /// Create a new ColourRange
    ///
    /// Given a lower bound, a step and a set of colours we can map any float value to our LED
    /// array.
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
                num_pixels: NUM_PIXELS as u8,
            })
        }
    }

    /// Get the colours that should be used for each LED.
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

    /// Return colours for all LEDs set to the same colour.
    pub fn all(&self, colour: Colour) -> Vec<Colour> {
        vec![colour; self.num_pixels as usize]
    }
}

type ResponseSender = tokio::sync::oneshot::Sender<Result<(), String>>;
type Request = (LEDCommand, ResponseSender);
type RequestSender = std::sync::mpsc::SyncSender<Request>;
type RequestReceiver = std::sync::mpsc::Receiver<Request>;

enum LEDCommand {
    Show([Colour; NUM_PIXELS], f32),
}

struct BlinktBackgroundLEDs {
    sender: RequestSender,
}

impl BlinktBackgroundLEDs {
    pub fn new() -> Self {
        // TODO: check if this should be 0
        let (req_sender, req_receiver) = sync_channel(0);

        thread::spawn(move || {
            run_worker(req_receiver);
        });

        BlinktBackgroundLEDs { sender: req_sender }
    }

    async fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String> {
        let (resp_sender, resp_receiver) = tokio::sync::oneshot::channel();
        let colours: [Colour; 8] = colours.try_into().expect("Invalid colour slice size");
        self.sender
            .try_send((LEDCommand::Show(colours, brightness), resp_sender))
            .expect("Could not request LED update");
        resp_receiver.await.unwrap()
    }
}

fn run_worker(requests: RequestReceiver) {
    let mut leds = BlinktLEDs::new();

    for (command, sender) in requests.iter() {
        match command {
            LEDCommand::Show(colours, brightness) => {
                sender.send(leds.show(&colours, brightness)).unwrap();
            }
        }
    }
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
                if (current.1 - brightness).abs() > f32::EPSILON {
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

    fn show(&mut self, colours: &[Colour], brightness: f32) -> Result<(), String> {
        if self.should_update(colours, brightness) {
            let mut colours_array: [Colour; NUM_PIXELS] = Default::default();
            colours_array.copy_from_slice(colours);
            let brightnesses = get_blinkt_brightness(&colours_array, brightness);
            let details = colours.iter().enumerate().zip(brightnesses.iter());

            for ((pixel, colour), &brightness) in details {
                self.blinkt
                    .set_pixel_rgbb(pixel, colour.0, colour.1, colour.2, brightness);
            }

            if let Err(err) = self.blinkt.show() {
                return Err(format!("Failed to write LEDs: {:?}", err));
            }
        }

        Ok(())
    }
}

fn get_pivot(colours: &[Colour; NUM_PIXELS]) -> usize {
    for i in 1..NUM_PIXELS {
        if colours[i - 1] != colours[i] {
            return i;
        }
    }
    0
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
pub(self) fn get_blinkt_brightness(
    colours: &[Colour; NUM_PIXELS],
    brightness: f32,
) -> [f32; NUM_PIXELS] {
    let pivot = get_pivot(colours);
    let x = 0.04;
    let o = 0.0;
    if (brightness + f32::EPSILON) < 0.01 {
        [0.0; NUM_PIXELS]
    } else if (brightness + f32::EPSILON) < 0.02 {
        match pivot {
            0 => [x, o, o, o, o, o, o, x],
            1 => [x, x, o, o, o, o, o, o],
            2 => [x, o, x, o, o, o, o, o],
            3 => [x, o, o, x, o, o, o, o],
            4 => [x, o, o, o, x, o, o, o],
            5 => [x, o, o, o, o, x, o, o],
            6 => [x, o, o, o, o, o, x, o],
            7 => [x, o, o, o, o, o, o, x],
            _ => unreachable!("pivot cannot be more than 7"),
        }
    } else if (brightness + f32::EPSILON) < 0.03 {
        match pivot {
            0 => [x, o, o, o, x, o, o, x],
            1 => [x, x, o, o, o, o, o, x],
            2 => [x, o, x, o, o, o, o, x],
            3 => [x, o, o, x, o, o, o, x],
            4 => [x, o, o, o, x, o, o, x],
            5 => [x, o, o, o, o, x, o, x],
            6 => [x, o, o, o, o, o, x, x],
            7 => [x, o, o, o, o, o, x, x],
            _ => unreachable!("pivot cannot be more than 7"),
        }
    } else if (brightness + f32::EPSILON) < 0.04 {
        match pivot {
            0 => [x, o, o, x, x, o, o, x],
            1 => [x, x, x, o, o, o, o, x],
            2 => [x, x, x, o, o, o, o, x],
            3 => [x, o, x, x, o, o, o, x],
            4 => [x, o, o, x, x, o, o, x],
            5 => [x, o, o, o, x, x, o, x],
            6 => [x, o, o, o, o, x, x, x],
            7 => [x, o, o, o, o, x, x, x],
            _ => unreachable!("pivot cannot be more than 7"),
        }
    } else {
        [brightness; NUM_PIXELS]
    }
}

impl Default for BlinktLEDs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod colour_range {
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
    }

    #[test]
    fn colour_bucket_ordering() {
        let bucket1 = ColourBucket::new("first", 1.1, COLOUR_BLUE);
        let bucket2 = ColourBucket::new("second", 2.2, COLOUR_ORANGE);
        let bucket3 = ColourBucket::new("third", 1.1, COLOUR_RED);

        assert!(bucket1 < bucket2);
        assert!(bucket2 > bucket3);
        assert_eq!(bucket1.cmp(&bucket3), Ordering::Equal);
        assert!(bucket1 != bucket3);
    }

    #[test]
    fn getting_pivot() {
        assert_eq!(get_pivot(&[COLOUR_BLUE; NUM_PIXELS]), 0);
        assert_eq!(
            get_pivot(&[
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
            ]),
            3
        );
        assert_eq!(
            get_pivot(&[
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_ORANGE,
                COLOUR_ORANGE,
            ]),
            6
        );
        assert_eq!(
            get_pivot(&[
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_BLUE,
                COLOUR_ORANGE,
            ]),
            7
        );
    }

    #[test]
    fn get_blinkt_brightness_when_off() {
        assert_eq!(
            get_blinkt_brightness(
                &[
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_ORANGE,
                ],
                0.005
            ),
            [0.0; 8]
        );
    }

    #[test]
    fn get_blinkt_brightness_when_two_leds() {
        assert_eq!(
            get_blinkt_brightness(
                &[
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_ORANGE,
                    COLOUR_ORANGE,
                ],
                0.01
            ),
            [0.04, 0.0, 0.0, 0.0, 0.0, 0.0, 0.04, 0.0]
        );
    }

    #[test]
    fn get_blinkt_brightness_when_three_leds() {
        assert_eq!(
            get_blinkt_brightness(
                &[
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_ORANGE,
                    COLOUR_ORANGE,
                ],
                0.02
            ),
            [0.04, 0.0, 0.0, 0.0, 0.0, 0.0, 0.04, 0.04]
        );
    }

    #[test]
    fn get_blinkt_brightness_when_four_leds() {
        assert_eq!(
            get_blinkt_brightness(
                &[
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_ORANGE,
                    COLOUR_ORANGE,
                ],
                0.03
            ),
            [0.04, 0.0, 0.0, 0.0, 0.0, 0.04, 0.04, 0.04]
        );
    }

    #[test]
    fn get_blinkt_brightness_when_on() {
        assert_eq!(
            get_blinkt_brightness(
                &[
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_BLUE,
                    COLOUR_ORANGE,
                    COLOUR_ORANGE,
                ],
                0.04
            ),
            [0.04; 8]
        );
    }

    #[test]
    fn brightness_next_from() {
        assert_eq!(Brightness::next_from(0.0), Brightness::Dim);
        assert_eq!(Brightness::next_from(0.009), Brightness::Dim);
        assert_eq!(Brightness::next_from(0.01), Brightness::Bright);
        assert_eq!(Brightness::next_from(0.49), Brightness::Bright);
        assert_eq!(Brightness::next_from(0.5), Brightness::Off);
        assert_eq!(Brightness::next_from(0.9), Brightness::Off);
    }
}
