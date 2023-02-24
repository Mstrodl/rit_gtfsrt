use gtfs_rt::{FeedHeader, FeedMessage};
use std::time::{SystemTime, UNIX_EPOCH};
use prost::Message;
use tide::{Request, Response};
use snafu::prelude::*;

pub async fn rit_protobuf(_req: Request<()>) -> tide::Result {
  Ok(
    Response::builder(200)
      .body(Message::encode_to_vec(&get_feed().await?))
      .content_type("application/vnd.google.protobuf")
      .build()
  )
}

#[derive(Debug, Snafu)]
pub enum GenFeedError {
  HttpError,
}

pub async fn get_feed() -> Result<FeedMessage, GenFeedError> {
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
    entity: vec![],
  })
}
