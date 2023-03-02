use crate::rit_protobuf::request;
use crate::rit_protobuf::GenFeedError;
use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use chrono_tz::{America::New_York, Tz};
use gtfs_rt::{TripDescriptor, trip_descriptor::ScheduleRelationship};
use serde::de;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::cmp;
use std::collections::HashMap;
use std::future::join;
use std::io::Cursor;
use zip::ZipArchive;

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
  success: bool,
}

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
  #[serde(deserialize_with = "day_time_deserializer")]
  departure_time: (String, u64),
  pub stop_id: u64,
  pub stop_sequence: u32,
}

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
  // println!("{hour}:{minute}:{second} = {value}");
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
  exact_times: u8,
}

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
  zip: ZipArchive<Cursor<Vec<u8>>>,
  routes: HashMap<u64, Route>,
  csv_routes: HashMap<String, CSVRoute>,
  csv_stop_times: Vec<StopTime>,
  csv_trips: Vec<CSVTrip>,
  csv_stops: HashMap<String, CSVStop>,
  csv_frequencies: HashMap<u64, CSVFrequency>,
  pub arrivals: Vec<Arrival>,
  pub vehicles: HashMap<u64, Vehicle>,
  stops: HashMap<u64, Stop>,
}
pub async fn get_schedule() -> Result<Schedule, GenFeedError> {
  let bytes = reqwest::get("https://api.transloc.com/gtfs/rit.zip")
    .await
    .map_err(|err| GenFeedError::Http(err, "https://api.transloc.com/gtfs/rit.zip".to_owned()))?
    .bytes()
    .await
    .map_err(|err| GenFeedError::Http(err, "https://api.transloc.com/gtfs/rit.zip".to_owned()))?;
  let bytes = Vec::from(bytes);
  let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(GenFeedError::Zip)?;

  let (stops, routes, vehicle_statuses) = join!(
    async {
      request::<StopOutput>("https://feeds.transloc.com/3/stops?include_routes=true&agencies=643")
        .await
    },
    async { request::<RouteOutput>("https://feeds.transloc.com/3/routes?agencies=643").await },
    async {
      request::<VehicleStatuses>(
        "https://feeds.transloc.com/3/vehicle_statuses?agencies=643&include_arrivals=true",
      )
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
  let stops = HashMap::from_iter(stops.stops.into_iter().map(|stop| (stop.id, stop)));
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
    zip,
    routes,
    stops,
    csv_routes,
    csv_stop_times,
    csv_frequencies,
    csv_stops,
    csv_trips,
    arrivals: vehicle_statuses.arrivals,
    vehicles,
  })
}

fn nearby(real_time: u64, seconds: (String, u64)) -> bool {
  let (orig, seconds) = seconds;
  // let seconds = seconds / 1000;
  let delta = (real_time as i64) - (seconds as i64);
  // let minutes = (real_time as i64) - SystemTime::now().duration_since(UNIX_EPOCH).as_secs()) / 60;
  println!("Nearby? Delta is {delta}. Real={real_time}, Schedule={seconds} ({orig})");
  delta < 60 * 10 && delta > -60 * 10
}

fn get_arrival_time(arrival: &Arrival) -> (DateTime<Tz>, u64) {
  let date = Utc
    .timestamp_opt(arrival.timestamp, 0)
    .single()
    .expect("Invalid arrival timestamp?");
  let date = date.with_timezone(&New_York);
  println!("Arriving @ {date}");
  // Get current time as a NaiveDateTime
  // let now = Local::now();
  // let now = NaiveDateTime::new(now.date().naive_local(), now.time());
  // println!("Present time is {now}");
  // let delta = arrival_time - now;
  // let seconds = delta.num_seconds();
  // println!("Getting arrival time: {} minutes from now (abs={})", seconds / 60, arrival.timestamp);

  let secs: u64 = date.num_seconds_from_midnight().into();
  println!("secs={secs}");
  if date.hour() < 4 {
    // We want to look at yesterday!
    (date, secs + Duration::days(1).num_seconds() as u64)
  } else {
    (date, secs)
  }
}

impl Schedule {
  pub fn find_trip_id(&self, arrival: &Arrival) -> Option<(TripDescriptor, StopTime)> {
    let route = self.routes.get(&arrival.route_id)?;
    let csv_route = self.csv_routes.get(&route.long_name)?;
    let stop = route.stops.iter().find(|stop| stop.id == arrival.stop_id)?;
    let csv_stop = self.csv_stops.get(&stop.code)?;
    let (arrival_date, arrival_time) = get_arrival_time(arrival);

    println!();
    for stop_time in &self.csv_stop_times {
      if stop_time.stop_id == 20 {
        println!("Stop time={:?}", stop_time);
      }
    }

    println!();
    println!();
    println!();
    println!("Found likely route: {}", csv_route.route_long_name);
    for trip in &self.csv_trips {
      if csv_route.route_id == trip.route_id {
        println!("Found a probable trip: {:?}", trip);
        let frequency = match self.csv_frequencies.get(&trip.trip_id) {
          Some(frequency) => frequency,
          None => {
            // TODO: Handle one-off trips
            println!("No frequency for trip {}", trip.trip_id);
            continue;
          }
        };

        for stop_time in &self.csv_stop_times {
          if stop_time.trip_id != trip.trip_id {
            continue;
          }

          if frequency.start_time.1 > arrival_time || frequency.end_time.1 < arrival_time {
            continue;
          }
          let relative_time = (stop_time.arrival_time.1 as i64) - (frequency.start_time.1 as i64);
          let trip_iteration: f64 =
            (arrival_time - stop_time.arrival_time.1) as f64 / frequency.headway_secs as f64;
          let trip_iteration = cmp::max(trip_iteration.round() as u64, 0);
          println!(
            "Chom.. {}",
            day_time_serializer(frequency.headway_secs * trip_iteration + frequency.start_time.1,)
          );
          return Some((
            TripDescriptor {
              trip_id: Some(trip.trip_id.to_string()),
              route_id: Some(trip.route_id.to_string()),
              direction_id: None,
              start_time: Some(day_time_serializer(
                frequency.headway_secs * trip_iteration + frequency.start_time.1,
              )),
              start_date: None,
              schedule_relationship: Some(ScheduleRelationship::Scheduled.into()),
            },
            stop_time.clone(),
          ));

          // if stop_time.trip_id == trip.trip_id
          //   && stop_time.stop_id == csv_stop.stop_id
          //   && nearby(arrival_time, stop_time.arrival_time.clone())
          // {
          //   // trip and stop_time belong to us!
          //   return Some((
          //     TripDescriptor {
          //       trip_id: Some(trip.trip_id.to_string()),
          //       route_id: Some(trip.route_id.to_string()),
          //       direction_id: None,
          //       start_time: None,
          //       start_date: None,
          //       schedule_relationship: None,
          //     },
          //     stop_time.clone(),
          //   ));
          // }
        }
      }
    }
    println!("Missing trip?! {:?} Stop={:?}", arrival, csv_stop);
    println!();
    println!();
    println!();
    None
  }
}
