use crate::protobuf_route::request;
use crate::protobuf_route::GenFeedError;
use crate::traits::Translate;
use chrono::DateTime;
use gtfs_rt::{
  alert::{Cause, Effect},
  Alert, EntitySelector, FeedEntity, TimeRange,
};
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Announcements {
  announcements: Vec<Announcement>,
  success: bool,
}
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Announcement {
  agency_id: u64,
  date: String,
  has_content: bool,
  html: String,
  id: u64,
  start_at: String,
  title: String,
  urgent: bool,
}

pub async fn get_alerts(agency_id: u64) -> Result<Vec<FeedEntity>, GenFeedError> {
  let announcements = match request::<Announcements>(&format!(
    "https://feeds.transloc.com/3/announcements?contents=true&agencies={agency_id}"
  ))
  .await
  {
    Ok(announcements) => announcements,
    Err(err) => {
      log::error!("Couldn't request announcements: {err}");
      return Ok(vec![]);
    }
  };
  Ok(
    announcements
      .announcements
      .into_iter()
      .map(|announcement| FeedEntity {
        id: announcement.id.to_string(),
        is_deleted: None,
        trip_update: None,
        vehicle: None,
        alert: Some(Alert {
          active_period: vec![TimeRange {
            start: DateTime::parse_from_rfc3339(&announcement.start_at)
              .map(|start| start.timestamp() as u64)
              .ok(),
            end: None,
          }],
          informed_entity: vec![EntitySelector {
            agency_id: Some(agency_id.to_string()),
            route_id: None,
            route_type: None,
            trip: None,
            stop_id: None,
          }],
          cause: Some(Cause::UnknownCause.into()), // UNKNOWN
          effect: Some(Effect::UnknownEffect.into()), // UNKNOWN
          url: None,
          header_text: Some(announcement.title.into_translation()),
          description_text: Some(announcement.html.into_translation()),
        }),
      })
      .collect(),
  )
}
