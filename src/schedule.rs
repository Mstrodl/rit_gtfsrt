use crate::rit_protobuf::GenFeedError;
use reqwest;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::io::Cursor;
use zip::ZipArchive;
use std::future::join;

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

async fn request<T: DeserializeOwned>(url: &str) -> Result<T, GenFeedError> {
  reqwest::get(url)
    .await
    .map_err(|_| GenFeedError::HttpError)?
    .json::<T>()
    .await
    .map_err(|_| GenFeedError::ParseError)
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

pub struct Schedule {
  zip: ZipArchive<Cursor<Vec<u8>>>,
  routes: Vec<Route>,
  csv_routes: Vec<CSVRoute>,
}
pub async fn get_schedule() -> Result<Schedule, GenFeedError> {
  let bytes = reqwest::get("https://api.transloc.com/gtfs/rit.zip")
    .await
    .map_err(|_| GenFeedError::HttpError)?
    .bytes()
    .await
    .map_err(|_| GenFeedError::HttpError)?;
  let bytes = Vec::from(bytes);
  let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| GenFeedError::ZipError)?;

  let (stops, routes) = join!(
    async {request::<StopOutput>("https://feeds.transloc.com/3/stops?include_routes=true&agencies=643").await},
    async {request::<RouteOutput>("https://feeds.transloc.com/3/routes?agencies=643").await}
  ).await;
  let stops = stops?;
  let routes = routes?;

  let routes = routes
    .routes
    .into_iter()
    .map(|route| {
      let thin_route = stops
        .routes
        .iter()
        .find(|other| other.id == route.id)
        .expect(&format!("Route {} doesn't exist on /stops?", route.id));
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
    .collect();

  let csv_routes: Vec<CSVRoute> = read_csv(zip, "routes.txt")?;

  Ok(Schedule { zip, routes })
}

impl Schedule {
  pub async fn find_trip_id() {}
}
