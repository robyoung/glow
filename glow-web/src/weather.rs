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
use serde::{Deserialize, Serialize};

use crate::store::{Store, StorePool};
use futures::join;

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
        let (observation, forecast) = join!(self.weather.observation(), self.weather.forecast());
        self.pool
            .get()
            .and_then(|store| {
                store.add_observation(&observation?)?;
                forecast?
                    .iter()
                    .map(|forecast| store.add_forecast(forecast))
                    .collect::<Result<Vec<()>>>()?;
                Ok(())
            })
            .unwrap_or_else(|err| error!("{}", err));
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub temperature: u32,
    pub humidity: u32,
    pub wind_speed: u32,
    pub wind_direction: WindDirection,
    pub date_time: DateTime<Utc>,
    pub point: Coord,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forecast {
    pub max_temperature: Option<u32>,
    pub min_temperature: u32,
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
    async fn forecast(&self) -> Result<[Forecast; 3]>;
}

#[derive(Clone)]
pub struct BBCWeatherService<G: UrlGetter> {
    location: String,
    getter: G,
}

const BBC_WEATHER_OBSERVATION_URL: &str =
    "https://weather-broker-cdn.api.bbci.co.uk/en/observation/rss/";
const BBC_WEATHER_FORECAST_URL: &str =
    "https://weather-broker-cdn.api.bbci.co.uk/en/forecast/rss/3day/";

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

    fn forecast_url(&self) -> String {
        let url = BBC_WEATHER_FORECAST_URL.to_owned();
        url + &self.location
    }
}

lazy_static! {
    static ref ELEMENT_NAMES: HashSet<&'static str> = vec!["description", "date", "link", "point"]
        .iter()
        .cloned()
        .collect();
}

#[async_trait]
impl<G: UrlGetter> WeatherService for BBCWeatherService<G> {
    #[allow(clippy::filter_map)]
    async fn observation(&self) -> Result<Observation> {
        let data = self.getter.get(&self.observation_url()).await?;

        let doc = roxmltree::Document::parse(std::str::from_utf8(&data)?)?;

        doc.descendants()
            // keeping filter and map separate here is clearer
            .filter(|n| n.has_tag_name("item"))
            .map(|item| {
                item.descendants()
                    .filter(|n| n.is_element() && ELEMENT_NAMES.contains(n.tag_name().name()))
                    .map(|n| (n.tag_name().name(), n.text().unwrap_or("")))
                    .collect::<HashMap<&str, &str>>()
            })
            .map(|parts| -> Result<Observation> {
                let (temperature, humidity, wind_speed, wind_direction) =
                    parse_observation_description(&parts)
                        .wrap_err("failed to parse description")?;

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
            })
            .next()
            .ok_or_else(|| eyre!("no observation found"))?
    }

    #[allow(clippy::filter_map)]
    async fn forecast(&self) -> Result<[Forecast; 3]> {
        let data = self.getter.get(&self.forecast_url()).await?;

        let doc = roxmltree::Document::parse(std::str::from_utf8(&data)?)?;
        let mut items = doc
            .descendants()
            // keeping filter and map separate here is clearer
            .filter(|node| node.has_tag_name("item"))
            .map(|item| {
                item.descendants()
                    .filter(|node| {
                        node.is_element() && ELEMENT_NAMES.contains(node.tag_name().name())
                    })
                    .map(|node| (node.tag_name().name(), node.text().unwrap_or("")))
                    .collect::<HashMap<&str, &str>>()
            })
            .map(|parts| -> Result<Forecast> {
                let (max_temperature, min_temperature, humidity, wind_speed, wind_direction) =
                    parse_forecast_description(&parts).wrap_err("failed to parse description")?;

                Ok(Forecast {
                    min_temperature,
                    max_temperature,
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
            })
            .collect::<Result<Vec<Forecast>>>()?;

        if items.len() == 3 {
            // Can't seem to get TryInto working because Forecast isn't Copy
            Ok([items.remove(0), items.remove(0), items.remove(0)])
        } else {
            Err(eyre!("wrong number of items found: {}", items.len()))
        }
    }
}

#[allow(clippy::non_ascii_literal)]
fn parse_observation_description(
    parts: &HashMap<&str, &str>,
) -> Result<(u32, u32, u32, WindDirection)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^Temperature: (\d+)°C \(\d+°F\), ",
            r"Wind Direction: ([\w ]+), Wind Speed: (\d+)mph, ",
            r"Humidity: (\d+)%,",
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

#[allow(clippy::non_ascii_literal)]
fn parse_forecast_description(
    parts: &HashMap<&str, &str>,
) -> Result<(Option<u32>, u32, u32, u32, WindDirection)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"^(?:Maximum Temperature: (\d+)°C \(\d+°F\), )?",
            r"Minimum Temperature: (\d+)°C \(\d+°F\), ",
            r"Wind Direction: ([\w ]+), Wind Speed: (\d+)mph, ",
            r"Visibility: [^,]*, Pressure: \d+mb, ",
            r"Humidity: (\d+)%,",
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
        captures
            .get(1)
            .map(|v| v.as_str().parse::<u32>())
            .transpose()?,
        captures.get(2).unwrap().as_str().parse::<u32>()?,
        captures.get(5).unwrap().as_str().parse::<u32>()?,
        captures.get(4).unwrap().as_str().parse::<u32>()?,
        captures.get(3).unwrap().as_str().parse::<WindDirection>()?,
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
    async fn get_observation() {
        let data = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss xmlns:atom="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:georss="http://www.georss.org/georss" version="2.0">
  <channel>
    <title>BBC Weather - Observations for  Land's End Airport, GB</title>
    <link>https://www.bbc.co.uk/weather/7668205</link>
    <description>Latest observations for Land's End Airport from BBC Weather, including weather, temperature and wind information</description>
    <language>en</language>
    <copyright>Copyright: (C) British Broadcasting Corporation, see http://www.bbc.co.uk/terms/additional_rss.shtml for more details</copyright>
    <pubDate>Tue, 07 Jul 2020 15:00:00 GMT</pubDate>
    <dc:date>2020-07-07T15:00:00Z</dc:date>
    <dc:language>en</dc:language>
    <dc:rights>Copyright: (C) British Broadcasting Corporation, see http://www.bbc.co.uk/terms/additional_rss.shtml for more details</dc:rights>
    <atom:link href="https://weather-service-thunder-broker.api.bbci.co.uk/en/observation/rss/7668205" type="application/rss+xml" rel="self" />
    <item>
      <title>Tuesday - 16:00 BST: Not available, 15°C (59°F)</title>
      <link>https://www.bbc.co.uk/weather/7668205</link>
      <description>Temperature: 15°C (59°F), Wind Direction: South Westerly, Wind Speed: 12mph, Humidity: 82%, Pressure: 1022mb, Steady, Visibility: --</description>
      <pubDate>Tue, 07 Jul 2020 15:00:00 GMT</pubDate>
      <guid isPermaLink="false">https://www.bbc.co.uk/weather/7668205-2020-07-07T16:00:00.000+01:00</guid>
      <dc:date>2020-07-07T15:00:00Z</dc:date>
      <georss:point>50.1028 -5.6706</georss:point>
    </item>
  </channel>
</rss>"#;
        let service =
            BBCWeatherService::with_getter("test", TestUrlGetter::new(data.as_bytes().to_owned()));

        let observation = service.observation().await.unwrap();

        assert_eq!(observation.temperature, 15);
        assert_eq!(observation.wind_direction, WindDirection::SouthWesterly);
    }

    #[tokio::test]
    async fn get_forecast() {
        let data = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss xmlns:atom="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:georss="http://www.georss.org/georss" version="2.0">
  <channel>
    <title>BBC Weather - Forecast for  Land's End Airport, GB</title>
    <link>https://www.bbc.co.uk/weather/7668205</link>
    <description>3-day forecast for Land's End Airport from BBC Weather, including weather, temperature and wind information</description>
    <language>en</language>
    <copyright>Copyright: (C) British Broadcasting Corporation, see http://www.bbc.co.uk/terms/additional_rss.shtml for more details</copyright>
    <pubDate>Tue, 07 Jul 2020 15:04:25 GMT</pubDate>
    <dc:date>2020-07-07T15:04:25Z</dc:date>
    <dc:language>en</dc:language>
    <dc:rights>Copyright: (C) British Broadcasting Corporation, see http://www.bbc.co.uk/terms/additional_rss.shtml for more details</dc:rights>
    <atom:link href="https://weather-broker-cdn.api.bbci.co.uk/%s/forecast/rss/3day/%s" type="application/rss+xml" rel="self" />
    <image>
      <title>BBC Weather - Forecast for  Land's End Airport, GB</title>
      <url>http://static.bbci.co.uk/weather/0.3.203/images/icons/individual_57_icons/en_on_light_bg/3.gif</url>
      <link>https://www.bbc.co.uk/weather/7668205</link>
    </image>
    <item>
      <title>Today: Sunny Intervals, Minimum Temperature: 13°C (56°F) Maximum Temperature: 16°C (61°F)</title>
      <link>https://www.bbc.co.uk/weather/7668205?day=0</link>
      <description>Minimum Temperature: 13°C (56°F), Wind Direction: South Westerly, Wind Speed: 18mph, Visibility: Good, Pressure: 1022mb, Humidity: 79%, UV Risk: 5, Pollution: Low, Sunrise: 05:22 BST, Sunset: 21:33 BST</description>
      <pubDate>Tue, 07 Jul 2020 15:04:25 GMT</pubDate>
      <guid isPermaLink="false">https://www.bbc.co.uk/weather/7668205-0-2020-07-07T09:57:00.000+0000</guid>
      <dc:date>2020-07-07T15:04:25Z</dc:date>
      <georss:point>50.1028 -5.6706</georss:point>
    </item>
    <item>
      <title>Wednesday: Thick Cloud, Minimum Temperature: 14°C (57°F) Maximum Temperature: 16°C (61°F)</title>
      <link>https://www.bbc.co.uk/weather/7668205?day=1</link>
      <description>Maximum Temperature: 16°C (61°F), Minimum Temperature: 14°C (57°F), Wind Direction: Westerly, Wind Speed: 17mph, Visibility: Poor, Pressure: 1018mb, Humidity: 97%, UV Risk: 1, Pollution: Low, Sunrise: 05:23 BST, Sunset: 21:32 BST</description>
      <pubDate>Tue, 07 Jul 2020 15:04:25 GMT</pubDate>
      <guid isPermaLink="false">https://www.bbc.co.uk/weather/7668205-1-2020-07-07T09:57:00.000+0000</guid>
      <dc:date>2020-07-07T15:04:25Z</dc:date>
      <georss:point>50.1028 -5.6706</georss:point>
    </item>
    <item>
      <title>Thursday: Drizzle, Minimum Temperature: 11°C (53°F) Maximum Temperature: 16°C (61°F)</title>
      <link>https://www.bbc.co.uk/weather/7668205?day=2</link>
      <description>Maximum Temperature: 16°C (61°F), Minimum Temperature: 11°C (53°F), Wind Direction: Westerly, Wind Speed: 15mph, Visibility: Moderate, Pressure: 1016mb, Humidity: 95%, UV Risk: 1, Pollution: Low, Sunrise: 05:24 BST, Sunset: 21:31 BST</description>
      <pubDate>Tue, 07 Jul 2020 15:04:25 GMT</pubDate>
      <guid isPermaLink="false">https://www.bbc.co.uk/weather/7668205-2-2020-07-07T09:57:00.000+0000</guid>
      <dc:date>2020-07-07T15:04:25Z</dc:date>
      <georss:point>50.1028 -5.6706</georss:point>
    </item>
  </channel>
</rss>"#;

        let service =
            BBCWeatherService::with_getter("test", TestUrlGetter::new(data.as_bytes().to_owned()));
        let forecast = service.forecast().await.unwrap();

        assert_eq!(forecast.len(), 3);
        assert_eq!(forecast[0].max_temperature, None);
        assert_eq!(forecast[0].min_temperature, 13);
        assert_eq!(forecast[1].max_temperature, Some(16));
    }
}
