use crate::protobuf_route::GenFeedError;
use crate::schedule::Schedule;
use gtfs_rt::{
  trip_update::{stop_time_update::ScheduleRelationship, StopTimeEvent, StopTimeUpdate},
  vehicle_position::VehicleStopStatus,
  FeedEntity, Position, TripUpdate, VehicleDescriptor, VehiclePosition,
};
use itertools::Itertools;
use std::iter::once;

fn mph_to_meters(mph: f32) -> f32 {
  mph * 0.44704
}

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
      .flat_map(|(arrival, trip_descriptor, stop_time)| {
        let vehicle = schedule
          .vehicles
          .get(&arrival.vehicle_id)
          .expect("Vehicle should exist for arrival");
        let time = StopTimeEvent {
          delay: None,
          uncertainty: Some(60),
          time: Some(arrival.timestamp),
        };
        let vehicle_descriptor = VehicleDescriptor {
          id: Some(arrival.vehicle_id.to_string()),
          label: Some(vehicle.call_name.clone()),
          license_plate: None,
        };
        let trip = FeedEntity {
          id: format!("{}-{}", stop_time.trip_id, arrival.timestamp),
          is_deleted: None,
          trip_update: Some(TripUpdate {
            trip: trip_descriptor.clone(),
            vehicle: Some(vehicle_descriptor.clone()),
            stop_time_update: vec![StopTimeUpdate {
              stop_sequence: Some(stop_time.stop_sequence),
              stop_id: Some(stop_time.stop_id.to_string()),
              arrival: Some(time.clone()),
              departure: Some(time),
              schedule_relationship: Some(ScheduleRelationship::Scheduled.into()),
            }],
            timestamp: Some(vehicle.timestamp / 1000),
            delay: None,
          }),
          vehicle: None,
          alert: None,
        };
        let vehicle = FeedEntity {
          id: format!("vehicle-{}", vehicle.id),
          is_deleted: None,
          trip_update: None,
          vehicle: Some(VehiclePosition {
            trip: Some(trip_descriptor),
            vehicle: Some(vehicle_descriptor),
            position: Some(Position {
              latitude: vehicle.position.0,
              longitude: vehicle.position.1,
              bearing: Some(vehicle.heading),
              odometer: None,
              speed: Some(mph_to_meters(vehicle.speed)),
            }),
            current_stop_sequence: Some(stop_time.stop_sequence),
            stop_id: Some(stop_time.stop_id.to_string()),
            current_status: Some(VehicleStopStatus::InTransitTo.into()),
            timestamp: Some(vehicle.timestamp / 1000),
            congestion_level: None,
            occupancy_status: None,
          }),
          alert: None,
        };
        once(vehicle).chain(once(trip))
      })
      .unique_by(|entity| entity.id.clone())
      .collect(),
  )
}
