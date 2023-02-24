use crate::alerts::get_alerts;
use gtfs_rt::{FeedEntity, FeedHeader, FeedMessage};
use prost::Message;
use snafu::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use tide::{Request, Response};

pub async fn rit_protobuf(_req: Request<()>) -> tide::Result {
  Ok(
    Response::builder(200)
      .body(Message::encode_to_vec(&get_feed().await?))
      .content_type("application/vnd.google.protobuf")
      .build(),
  )
}

#[derive(Debug, Snafu)]
pub enum GenFeedError {
  ParseError,
  HttpError,
  ZipError,
}

pub async fn get_feed() -> Result<FeedMessage, GenFeedError> {
  let alert = get_alerts().await?;
  let entity = alert;
  Ok(FeedMessage {
    header: FeedHeader {
      gtfs_realtime_version: "2.0".to_owned(),
      incrementality: None,
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
