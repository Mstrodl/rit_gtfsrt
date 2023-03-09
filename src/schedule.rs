use crate::protobuf_route::request;
use crate::protobuf_route::GenFeedError;
use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use chrono_tz::{America::New_York, Tz};
use gtfs_rt::{trip_descriptor::ScheduleRelationship, TripDescriptor};
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache};
use lazy_static::lazy_static;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::de;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::cmp;
use std::collections::HashMap;
use std::future::join;
use std::io::Cursor;
use zip::ZipArchive;

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
struct Stop {
  code: String,
  description: String,
  id: u64,
  location_type: String,
  name: String,
  position: [f64; 2],
  url: String,
}
#[derive(Debug, Deserialize)]
struct StopOutput {
  routes: Vec<ThinRawRoute>,
  stops: Vec<Stop>,
}
#[derive(Debug, Deserialize)]
struct RouteOutput {
  routes: Vec<RawRoute>,
  #[allow(dead_code)]
  success: bool,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RawRoute {
  agency_id: u64,
  color: String,
  description: String,
  id: u64,
  is_active: bool,
  long_name: String,
  short_name: String,
  text_color: String,
  r#type: String,
  url: String,
}

#[derive(Debug, Deserialize)]
struct ThinRawRoute {
  id: u64,
  stops: Vec<u64>,
}

#[derive(Debug)]
struct Route {
  long_name: String,
  id: u64,
  stops: Vec<Stop>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CSVRoute {
  route_id: u64,
  route_short_name: String,
  route_long_name: String,
  route_desc: String,
  route_url: String,
  route_color: String,
  route_text_color: String,
  route_type: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopTime {
  pub trip_id: u64,
  #[serde(deserialize_with = "day_time_deserializer")]
  arrival_time: (String, u64),
  #[allow(dead_code)]
  #[serde(deserialize_with = "day_time_deserializer")]
  departure_time: (String, u64),
  pub stop_id: u64,
  pub stop_sequence: u32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Arrival {
  agency_id: u64,
  call_name: String,
  distance: f64,
  headsign: Option<String>,
  route_id: u64,
  stop_id: u64,
  pub timestamp: i64,
  trip_id: Option<u64>,
  r#type: String,
  pub vehicle_id: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Vehicle {
  pub id: u64,
  pub call_name: String,
  current_stop_id: Option<u64>,
  pub heading: f32,
  pub load: f64,
  next_stop: Option<u64>,
  off_route: bool,
  pub position: (f32, f32),
  route_id: u64,
  segment_id: Option<u64>,
  pub speed: f32,
  stop_pattern_id: u64,
  pub timestamp: u64,
  trip_id: u64,
}

#[derive(Debug, Deserialize)]
struct VehicleStatuses {
  arrivals: Vec<Arrival>,
  vehicles: Vec<Vehicle>,
}

fn get_time_component<'de, D>(component: &str) -> Result<u64, D::Error>
where
  D: de::Deserializer<'de>,
{
  component.parse::<u64>().map_err(|err| {
    de::Error::custom(format!(
      "Failed to deserialize day_time: {} {}",
      component, err
    ))
  })
}

fn day_time_deserializer<'de, D>(deserializer: D) -> Result<(String, u64), D::Error>
where
  D: de::Deserializer<'de>,
{
  let time: String = Deserialize::deserialize(deserializer)?;
  let k = time.clone();
  let time_parts: Vec<&str> = k.splitn(3, ':').collect();
  let hour = get_time_component::<D>(time_parts[0])?;
  let minute = get_time_component::<D>(time_parts[1])?;
  let second = get_time_component::<D>(time_parts[2])?;
  // hour/minute/second to seconds:
  let value = hour * 3600 + minute * 60 + second;
  Ok((time, value))
}

fn day_time_serializer(total_seconds: u64) -> String {
  let seconds = total_seconds % 60;
  let total_minutes = total_seconds / 60;
  let minutes = total_minutes % 60;
  let hours = total_minutes / 60;
  format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn read_csv<T: DeserializeOwned>(
  zip: &mut ZipArchive<Cursor<Vec<u8>>>,
  path: &str,
) -> Result<Vec<T>, GenFeedError> {
  let file = zip.by_name(path).map_err(GenFeedError::Zip)?;
  let mut reader = csv::Reader::from_reader(file);
  let reader = reader.deserialize();
  Ok(reader.filter_map(|item| item.ok()).collect::<Vec<T>>())
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CSVTrip {
  trip_id: u64,
  route_id: u64,
  service_id: u64,
  trip_headsign: String,
  trip_short_name: String,
  direction_id: u64,
  shape_id: String,
  wheelchair_accessible: u64,
  bikes_allowed: u64,
  block_id: String,
  block_name: String,
}

#[derive(Debug, Deserialize)]
struct CSVFrequency {
  trip_id: u64,
  #[serde(deserialize_with = "day_time_deserializer")]
  start_time: (String, u64),
  #[serde(deserialize_with = "day_time_deserializer")]
  end_time: (String, u64),
  headway_secs: u64,
  #[allow(dead_code)]
  exact_times: u8,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CSVStop {
  stop_id: u64,
  stop_code: String,
  stop_name: String,
  stop_desc: String,
  stop_lat: f64,
  stop_lon: f64,
  stop_url: String,
  location_type: u64,
}

pub struct Schedule {
  // zip: ZipArchive<Cursor<Vec<u8>>>,
  routes: HashMap<u64, Route>,
  csv_routes: HashMap<String, CSVRoute>,
  csv_stop_times: Vec<StopTime>,
  csv_trips: Vec<CSVTrip>,
  csv_stops: HashMap<String, CSVStop>,
  csv_frequencies: HashMap<u64, CSVFrequency>,
  pub arrivals: Vec<Arrival>,
  pub vehicles: HashMap<u64, Vehicle>,
  transit_workaround: bool,
  // stops: HashMap<u64, Stop>,
}

lazy_static! {
  static ref CACHING_HTTP: ClientWithMiddleware = ClientBuilder::new(Client::new())
    .with(Cache(HttpCache {
      mode: CacheMode::Default,
      manager: CACacheManager::default(),
      options: None,
    }))
    .build();
}

pub async fn get_schedule(
  agency_id: u64,
  agency_code: &str,
  transit_workaround: bool,
) -> Result<Schedule, GenFeedError> {
  let bytes = CACHING_HTTP
    .get("https://api.transloc.com/gtfs/rit.zip")
    .send()
    .await
    .map_err(|err| {
      GenFeedError::ZipHttp(
        err,
        format!("https://api.transloc.com/gtfs/{agency_code}.zip"),
      )
    })?
    .bytes()
    .await
    .map_err(|err| {
      GenFeedError::Http(
        err,
        format!("https://api.transloc.com/gtfs/{agency_code}.zip"),
      )
    })?;
  let bytes = Vec::from(bytes);
  let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(GenFeedError::Zip)?;

  let (stops, routes, vehicle_statuses) = join!(
    async {
      request::<StopOutput>(&format!(
        "https://feeds.transloc.com/3/stops?include_routes=true&agencies={agency_id}"
      ))
      .await
    },
    async {
      request::<RouteOutput>(&format!(
        "https://feeds.transloc.com/3/routes?agencies={agency_id}"
      ))
      .await
    },
    async {
      request::<VehicleStatuses>(&format!(
        "https://feeds.transloc.com/3/vehicle_statuses?agencies={agency_id}&include_arrivals=true"
      ))
      .await
    }
  )
  .await;
  let stops = stops?;
  let routes = routes?;
  let vehicle_statuses = vehicle_statuses?;

  let routes = routes
    .routes
    .into_iter()
    .map(|route| {
      let thin_route = stops
        .routes
        .iter()
        .find(|other| other.id == route.id)
        .unwrap_or_else(|| panic!("Route {} doesn't exist on /stops?", route.id));
      let stops = stops
        .stops
        .iter()
        .filter(|stop| thin_route.stops.contains(&stop.id))
        .cloned()
        .collect();
      Route {
        long_name: route.long_name,
        id: route.id,
        stops,
      }
    })
    .map(|route| (route.id, route));
  let routes = HashMap::from_iter(routes);

  let csv_routes: Vec<CSVRoute> = read_csv(&mut zip, "routes.txt")?;
  let csv_stop_times: Vec<StopTime> = read_csv(&mut zip, "stop_times.txt")?;
  let csv_trips: Vec<CSVTrip> = read_csv(&mut zip, "trips.txt")?;
  let csv_routes = HashMap::from_iter(
    csv_routes
      .into_iter()
      .map(|route| (route.route_long_name.clone(), route)),
  );
  let csv_stops: Vec<CSVStop> = read_csv(&mut zip, "stops.txt")?;
  let csv_stops = HashMap::from_iter(
    csv_stops
      .into_iter()
      .map(|stop| (stop.stop_code.clone(), stop)),
  );
  let csv_frequencies: Vec<CSVFrequency> = read_csv(&mut zip, "frequencies.txt")?;
  let csv_frequencies = HashMap::from_iter(
    csv_frequencies
      .into_iter()
      .map(|frequency| (frequency.trip_id, frequency)),
  );
  let vehicles = HashMap::from_iter(
    vehicle_statuses
      .vehicles
      .into_iter()
      .map(|vehicle| (vehicle.id, vehicle)),
  );

  Ok(Schedule {
    routes,
    csv_routes,
    csv_stop_times,
    csv_frequencies,
    csv_stops,
    csv_trips,
    arrivals: vehicle_statuses.arrivals,
    vehicles,
    transit_workaround,
  })
}

fn nearby(real_time: u64, seconds: (String, u64)) -> bool {
  let seconds = seconds.1;
  let delta = (real_time as i64) - (seconds as i64);
  delta < 60 * 10 && delta > -60 * 10
}

fn get_arrival_time(arrival: &Arrival) -> (DateTime<Tz>, u64) {
  let date = Utc
    .timestamp_opt(arrival.timestamp, 0)
    .single()
    .expect("Invalid arrival timestamp?");
  let date = date.with_timezone(&New_York);

  let secs: u64 = date.num_seconds_from_midnight().into();
  if date.hour() < 4 {
    // We want to look at yesterday!
    (date, secs + Duration::days(1).num_seconds() as u64)
  } else {
    (date, secs)
  }
}

fn within_buffer(start_secs: u64, now: u64, end_secs: u64) -> bool {
  (start_secs - 60 * 10) < now && now < (end_secs + 60 * 10)
}

impl Schedule {
  pub fn find_trip_id(&self, arrival: &Arrival) -> Option<(TripDescriptor, StopTime)> {
    let route = self.routes.get(&arrival.route_id)?;
    let csv_route = self.csv_routes.get(&route.long_name)?;
    let stop = route.stops.iter().find(|stop| stop.id == arrival.stop_id)?;
    let csv_stop = self.csv_stops.get(&stop.code)?;
    let arrival_time = get_arrival_time(arrival).1;

    for trip in &self.csv_trips {
      if csv_route.route_id == trip.route_id {
        if let Some(frequency) = self.csv_frequencies.get(&trip.trip_id) {
          for stop_time in &self.csv_stop_times {
            if stop_time.trip_id != trip.trip_id {
              continue;
            }

            if !within_buffer(frequency.start_time.1, arrival_time, frequency.end_time.1) {
              continue;
            }
            let trip_iteration: f64 =
              (arrival_time - stop_time.arrival_time.1) as f64 / frequency.headway_secs as f64;
            let trip_iteration = cmp::max(trip_iteration.round() as u64, 0);
            let start_time = frequency.headway_secs * trip_iteration + frequency.start_time.1;
            return Some((
              TripDescriptor {
                trip_id: Some(if self.transit_workaround {
                  format!("{}_{}", trip.trip_id, start_time)
                } else {
                  trip.trip_id.to_string()
                }),
                route_id: Some(trip.route_id.to_string()),
                direction_id: None,
                start_time: if self.transit_workaround {
                  None
                } else {
                  Some(day_time_serializer(start_time))
                },
                start_date: None,
                schedule_relationship: Some(ScheduleRelationship::Scheduled.into()),
              },
              stop_time.clone(),
            ));
          }
        } else {
          for stop_time in &self.csv_stop_times {
            if stop_time.trip_id != trip.trip_id {
              continue;
            }
            if stop_time.trip_id == trip.trip_id
              && stop_time.stop_id == csv_stop.stop_id
              && nearby(arrival_time, stop_time.arrival_time.clone())
            {
              // trip and stop_time belong to us!
              return Some((
                TripDescriptor {
                  trip_id: Some(trip.trip_id.to_string()),
                  route_id: Some(trip.route_id.to_string()),
                  direction_id: None,
                  start_time: None,
                  start_date: None,
                  schedule_relationship: None,
                },
                stop_time.clone(),
              ));
            }
          }
        }
      }
    }
    eprintln!("Missing trip?! {:?} Stop={:?}", arrival, csv_stop);
    None
  }
}
