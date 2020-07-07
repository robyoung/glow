//! Pulling weather data
//!
//! Currently coming from the BBC
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    time::Duration,
};

use actix::prelude::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eyre::{eyre, Error, Result, WrapErr};
use hyper::body::HttpBody as _;
use hyper::Client;
use lazy_static::lazy_static;
use log::{error, info};
use regex::Regex;

use crate::store::StorePool;

#[derive(Clone)]
pub struct WeatherMonitor<P: StorePool, W: WeatherService> {
    pool: P,
    weather: W,
}

impl<P: StorePool + 'static, W: WeatherService + 'static> WeatherMonitor<P, W> {
    pub fn new(pool: P, weather: W) -> Self {
        Self { pool, weather }
    }

    async fn update(self) {
        match self.weather.observation().await {
            Ok(observation) => {
                info!("Received weather observation: {:?}", observation);
            }
            Err(err) => {
                error!("Failed to get weather observation: {:?}", err);
            }
        }
    }
}

impl<P: StorePool + 'static, W: WeatherService + 'static> Actor for WeatherMonitor<P, W> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        info!("Weather service is alive");

        ctx.spawn(actix::fut::wrap_future(self.clone().update()));

        ctx.run_interval(Duration::from_secs(60 * 60), move |act, ctx| {
            let fut = actix::fut::wrap_future(act.clone().update());
            ctx.spawn(fut);
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindDirection {
    Northerly,
    NorthNorthEasterly,
    NorthEasterly,
    EastNorthEasterly,
    Easterly,
    EastSouthEasterly,
    SouthEasterly,
    SouthSouthEasterly,
    Southerly,
    SouthSouthWesterly,
    SouthWesterly,
    WestSouthWesterly,
    Westerly,
    WestNorthWesterly,
    NorthWesterly,
    NorthNorthWesterly,
}

impl FromStr for WindDirection {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Northerly" => Ok(WindDirection::Northerly),
            "North North Easterly" => Ok(WindDirection::NorthNorthEasterly),
            "North Easterly" => Ok(WindDirection::NorthEasterly),
            "East North Easterly" => Ok(WindDirection::EastNorthEasterly),
            "Easterly" => Ok(WindDirection::Easterly),
            "East South Easterly" => Ok(WindDirection::EastSouthEasterly),
            "South Easterly" => Ok(WindDirection::SouthEasterly),
            "South South Easterly" => Ok(WindDirection::SouthSouthEasterly),
            "Southerly" => Ok(WindDirection::Southerly),
            "South South Westerly" => Ok(WindDirection::SouthSouthWesterly),
            "South Westerly" => Ok(WindDirection::SouthWesterly),
            "West South Westerly" => Ok(WindDirection::WestSouthWesterly),
            "Westerly" => Ok(WindDirection::Westerly),
            "West North Westerly" => Ok(WindDirection::WestNorthWesterly),
            "North Westerly" => Ok(WindDirection::NorthWesterly),
            "North North Westerly" => Ok(WindDirection::NorthNorthWesterly),
            _ => Err(eyre!("Could not parse {} as WindDirection", s)),
        }
    }
}

pub type Coord = (f32, f32);

#[derive(Debug)]
pub struct Observation {
    pub temperature: u32,
    pub humidity: u32,
    pub wind_speed: u32,
    pub wind_direction: WindDirection,
    pub date_time: DateTime<Utc>,
    pub point: Coord,
    pub url: String,
}

#[async_trait]
pub trait WeatherService: Unpin + Clone {
    async fn observation(&self) -> Result<Observation>;
    async fn forecast(&self);
}

#[derive(Clone)]
pub struct BBCWeatherService<G: UrlGetter> {
    location: String,
    getter: G,
}

const BBC_WEATHER_OBSERVATION_URL: &str =
    "https://weather-broker-cdn.api.bbci.co.uk/en/observation/rss/";

impl BBCWeatherService<HyperUrlGetter> {
    pub fn new(location: &str) -> Self {
        Self {
            location: location.to_string(),
            getter: HyperUrlGetter::default(),
        }
    }
}

impl<G: UrlGetter> BBCWeatherService<G> {
    #[cfg(test)]
    pub fn with_getter(location: &str, getter: G) -> Self {
        Self {
            location: location.to_string(),
            getter,
        }
    }

    fn observation_url(&self) -> String {
        let url = BBC_WEATHER_OBSERVATION_URL.to_owned();
        url + &self.location
    }
}

#[allow(clippy::non_ascii_literal)]
fn parse_description(parts: &HashMap<&str, &str>) -> Result<(u32, u32, u32, WindDirection)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^Temperature: (\d+)°C \(\d+°F\), ",
            r"Wind Direction: ([\w ]+), Wind Speed: (\d+)mph, ",
            r"Humidity: (\d+)%, ",
            r"Pressure: -- mb, (?:Not available)?, Visibility: --$"
        ))
        .unwrap();
    }

    let description = parts
        .get("description")
        .ok_or_else(|| eyre!("'description' not found"))?;
    let captures = RE
        .captures(description)
        .ok_or_else(|| eyre!("'description' did not match pattern: {}", description))?;

    Ok((
        captures.get(1).unwrap().as_str().parse::<u32>()?,
        captures.get(4).unwrap().as_str().parse::<u32>()?,
        captures.get(3).unwrap().as_str().parse::<u32>()?,
        captures.get(2).unwrap().as_str().parse::<WindDirection>()?,
    ))
}

fn parse_date(parts: &HashMap<&str, &str>) -> Result<DateTime<Utc>> {
    Ok(parts
        .get("date")
        .ok_or_else(|| eyre!("'date' not found"))?
        .parse::<DateTime<Utc>>()?)
}

fn parse_point(parts: &HashMap<&str, &str>) -> Result<(f32, f32)> {
    let coords = parts
        .get("point")
        .ok_or_else(|| eyre!("'point' not found"))?
        .split(' ')
        .map(|p| p.parse::<f32>().map_err(Error::from))
        .collect::<Result<Vec<f32>>>()?;

    if coords.len() == 2 {
        Ok((coords[0], coords[1]))
    } else {
        Err(eyre!("wrong number of points"))
    }
}

#[async_trait]
impl<G: UrlGetter> WeatherService for BBCWeatherService<G> {
    async fn observation(&self) -> Result<Observation> {
        let data = self.getter.get(&self.observation_url()).await?;

        let doc = roxmltree::Document::parse(std::str::from_utf8(&data)?)?;
        let item = doc.descendants().find(|n| n.has_tag_name("item")).unwrap();

        lazy_static! {
            static ref ELEMENT_NAMES: HashSet<&'static str> =
                vec!["description", "date", "link", "point"]
                    .iter()
                    .cloned()
                    .collect();
        }

        #[allow(clippy::filter_map)]
        let parts = item
            .descendants()
            .filter(|n| n.is_element() && ELEMENT_NAMES.contains(n.tag_name().name()))
            .map(|n| (n.tag_name().name(), n.text().unwrap_or("")))
            .collect::<HashMap<&str, &str>>();

        let (temperature, humidity, wind_speed, wind_direction) =
            parse_description(&parts).wrap_err("failed to parse description")?;

        Ok(Observation {
            temperature,
            humidity,
            wind_speed,
            wind_direction,
            date_time: parse_date(&parts).wrap_err("failed to parse date")?,
            point: parse_point(&parts).wrap_err("failed to parse point")?,
            url: parts
                .get("link")
                .ok_or_else(|| eyre!("Could not build Observation; 'link' not found"))?
                .to_owned()
                .to_owned(),
        })
    }

    async fn forecast(&self) {}
}

#[async_trait]
pub trait UrlGetter: Unpin + Clone + Default + Send + Sync {
    async fn get(&self, url: &str) -> Result<Vec<u8>>;
}

#[derive(Clone, Default)]
pub struct HyperUrlGetter {}

#[async_trait]
impl UrlGetter for HyperUrlGetter {
    async fn get(&self, url: &str) -> Result<Vec<u8>> {
        let client: Client<_, hyper::Body> =
            Client::builder().build(hyper_rustls::HttpsConnector::new());
        let mut resp = client.get(url.parse()?).await?;
        let mut data: Vec<u8> = vec![];
        while let Some(chunk) = resp.body_mut().data().await {
            data.extend(chunk?);
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[derive(Clone, Default, Debug)]
    struct TestUrlGetter {
        response: Vec<u8>,
    }

    impl TestUrlGetter {
        fn new(response: Vec<u8>) -> Self {
            Self { response }
        }
    }

    #[async_trait]
    impl UrlGetter for TestUrlGetter {
        async fn get(&self, _url: &str) -> Result<Vec<u8>> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn my_test() {
        let data = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss xmlns:atom="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:georss="http://www.georss.org/georss" version="2.0">
  <channel>
    <title>BBC Weather - Observations for  Chiswick, GB</title>
    <link>https://www.bbc.co.uk/weather/2653121</link>
    <description>Latest observations for Chiswick from BBC Weather, including weather, temperature and wind information</description>
    <language>en</language>
    <copyright>Copyright: (C) British Broadcasting Corporation, see http://www.bbc.co.uk/terms/additional_rss.shtml for more details</copyright>
    <pubDate>Mon, 06 Jul 2020 16:00:00 GMT</pubDate>
    <dc:date>2020-07-06T16:00:00Z</dc:date>
    <dc:language>en</dc:language>
    <dc:rights>Copyright: (C) British Broadcasting Corporation, see http://www.bbc.co.uk/terms/additional_rss.shtml for more details</dc:rights>
    <atom:link href="https://weather-service-thunder-broker.api.bbci.co.uk/en/observation/rss/2653121" type="application/rss+xml" rel="self" />
    <item>
      <title>Monday - 17:00 BST: Not available, 19°C (67°F)</title>
      <link>https://www.bbc.co.uk/weather/2653121</link>
      <description>Temperature: 19°C (67°F), Wind Direction: North Westerly, Wind Speed: 8mph, Humidity: 45%, Pressure: -- mb, , Visibility: --</description>
      <pubDate>Mon, 06 Jul 2020 16:00:00 GMT</pubDate>
      <guid isPermaLink="false">https://www.bbc.co.uk/weather/2653121-2020-07-06T17:00:00.000+01:00</guid>
      <dc:date>2020-07-06T16:00:00Z</dc:date>
      <georss:point>51.4927 -0.258</georss:point>
    </item>
  </channel>
</rss>"#;
        let service =
            BBCWeatherService::with_getter("test", TestUrlGetter::new(data.as_bytes().to_owned()));

        let observation = service.observation().await.unwrap();

        assert_eq!(observation.temperature, 19);
        assert_eq!(observation.wind_direction, WindDirection::NorthWesterly);
    }
}
