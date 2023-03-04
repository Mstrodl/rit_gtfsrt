use crate::rit_protobuf::request;
use crate::rit_protobuf::GenFeedError;
use crate::traits::Translate;
use chrono::DateTime;
use gtfs_rt::{Alert, EntitySelector, FeedEntity, TimeRange};
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

const RIT_AGENCY_ID: &str = "643";

pub async fn get_alerts() -> Result<Vec<FeedEntity>, GenFeedError> {
  let announcements = request::<Announcements>(&format!(
    "https://feeds.transloc.com/3/announcements?contents=true&agencies={RIT_AGENCY_ID}"
  ))
  .await?;
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
            agency_id: Some(RIT_AGENCY_ID.to_string()),
            route_id: None,
            route_type: None,
            trip: None,
            stop_id: None,
          }],
          cause: Some(1),  // UNKNOWN
          effect: Some(8), // UNKNOWN
          url: None,
          header_text: Some(announcement.title.into_translation()),
          description_text: Some(announcement.html.into_translation()),
        }),
      })
      .collect(),
  )
}
