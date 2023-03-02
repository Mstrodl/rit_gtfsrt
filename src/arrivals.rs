use crate::rit_protobuf::GenFeedError;
use crate::schedule::Schedule;
use gtfs_rt::{FeedEntity, TripDescriptor, TripUpdate, VehicleDescriptor, trip_update::{StopTimeUpdate, StopTimeEvent}};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn get_trip_arrivals(schedule: &Schedule) -> Result<Vec<FeedEntity>, GenFeedError> {
  Ok(
    schedule
      .arrivals
      .iter()
      .filter_map(|arrival| {
        schedule
          .find_trip_id(arrival)
          .map(|(trip_descriptor, stop_time)| (arrival, trip_descriptor, stop_time))
      })
      .map(|(arrival, trip_descriptor, stop_time)| {
        let time = StopTimeEvent {
              delay: None,
              uncertainty: Some(60),
              time: Some(arrival.timestamp),
            };
        FeedEntity {
        id: stop_time.trip_id.to_string(),
        is_deleted: None,
        trip_update: Some(TripUpdate {
          trip: trip_descriptor,
          vehicle: Some(VehicleDescriptor {
            id: Some(arrival.vehicle_id.to_string()),
            label: None,
            license_plate: None,
          }),
          stop_time_update: vec![StopTimeUpdate {
            stop_sequence: Some(stop_time.stop_sequence),
            stop_id: Some(stop_time.stop_id.to_string()),
            arrival: Some(time.clone()),
            departure: Some(time),
            schedule_relationship: None,
          }],
          timestamp: Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()),
          delay: None,
        }),
        vehicle: None,
        alert: None,
        }
      })
      .collect(),
  )
}
