use crate::alerts::get_alerts;
use crate::arrivals::get_trip_arrivals;
use crate::schedule::get_schedule;
use gtfs_rt::{feed_header::Incrementality, FeedEntity, FeedHeader, FeedMessage};
use prost::Message;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use tide::{Request, Response};
use zip::result::ZipError;

pub async fn protobuf_route(req: Request<()>) -> tide::Result {
  let agency_id: u64 = req
    .param("agency_id")
    .expect("missing agency_id url param")
    .parse()
    .expect("agency_id url param should be a u64");
  let agency_code = req
    .param("agency_code")
    .expect("missing agency_code url param");
  let feed = get_feed(agency_id, agency_code).await;
  // if let Err(GenFeedError::Http(err, url)) = &feed {
  //   println!("Errenous url: {:?}", err.url());
  // }
  // println!("Feed is: {:?}", feed);
  Ok(
    Response::builder(200)
      .body(Message::encode_to_vec(&feed?))
      .content_type("application/vnd.google.protobuf")
      .build(),
  )
}

pub async fn request<T: DeserializeOwned>(url: &str) -> Result<T, GenFeedError> {
  reqwest::get(url)
    .await
    .map_err(|err| GenFeedError::Http(err, url.to_string()))?
    .json::<T>()
    .await
    .map_err(|err| GenFeedError::Http(err, url.to_string()))
}

#[derive(Debug)]
pub enum GenFeedError {
  Zip(ZipError),
  Http(reqwest::Error, String),
}
impl Error for GenFeedError {}
impl fmt::Display for GenFeedError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Zip(err) => write!(f, "GenFeedError(Zip({err}))"),
      Self::Http(err, url) => write!(f, "GenFeedError(Http({err}, {url}))"),
    }
  }
}

pub async fn get_feed(agency_id: u64, agency_code: &str) -> Result<FeedMessage, GenFeedError> {
  let mut entity: Vec<FeedEntity> = vec![];
  let mut alert = get_alerts(agency_id).await?;
  entity.append(&mut alert);
  let schedule = get_schedule(agency_id, agency_code).await?;
  entity.append(&mut get_trip_arrivals(&schedule).await?);
  Ok(FeedMessage {
    header: FeedHeader {
      gtfs_realtime_version: "2.0".to_owned(),
      incrementality: Some(Incrementality::FullDataset.into()),
      timestamp: Some(
        SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .expect("Can't get time")
          .as_secs(),
      ),
    },
    entity,
  })
}
