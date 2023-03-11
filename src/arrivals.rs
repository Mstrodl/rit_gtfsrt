use crate::protobuf_route::GenFeedError;
use crate::schedule::{Schedule, get_arrival_time, day_time_serializer};
use gtfs_rt::{
  trip_update::{stop_time_update::ScheduleRelationship, StopTimeEvent, StopTimeUpdate},
  vehicle_position::VehicleStopStatus,
  FeedEntity, Position, TripUpdate, VehicleDescriptor, VehiclePosition,
};
use itertools::Itertools;
use std::iter::{once, Iterator};
use std::time::{SystemTime, UNIX_EPOCH};

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
      })
      .flat_map(|arrival_data| {
        println!("-----");
        let arrival = arrival_data.arrival;
        let (local_arrival_time_str, local_arrival_time) = get_arrival_time(&arrival);
        let vehicle = schedule.vehicles.get(&arrival.vehicle_id);
        let delta = (local_arrival_time as i64 - arrival_data.scheduled_arrival as i64) as i32;
        println!("Stop is {:?}", arrival_data.csv_stop);
        println!("Delta: {delta}");
        println!("{} / {:?} / {} vs scheduled: {:?}", arrival.timestamp, (local_arrival_time_str, local_arrival_time), day_time_serializer(local_arrival_time), (day_time_serializer(arrival_data.scheduled_arrival), arrival_data.scheduled_arrival));
        let time = StopTimeEvent {
          delay: Some(-delta),
          uncertainty: Some(60),
          //time: Some(arrival.timestamp),
          time: None,
        };
        let vehicle_descriptor = vehicle.map(|vehicle| VehicleDescriptor {
          id: Some(arrival.vehicle_id.to_string()),
          label: Some(vehicle.call_name.clone()),
          license_plate: None,
        });
        let trip = FeedEntity {
          id: format!("{}-{}", arrival_data.stop_time.trip_id, arrival.timestamp),
          is_deleted: None,
          trip_update: Some(TripUpdate {
            trip: arrival_data.trip_descriptor.clone(),
            vehicle: vehicle_descriptor.clone(),
            stop_time_update: vec![StopTimeUpdate {
              stop_sequence: Some(arrival_data.stop_time.stop_sequence),
              stop_id: Some(arrival_data.stop_time.stop_id.to_string()),
              arrival: Some(time.clone()),
              departure: Some(time),
              schedule_relationship: Some(ScheduleRelationship::Scheduled.into()),
            }],
            timestamp: Some(vehicle.map_or_else(
              || {
                SystemTime::now()
                  .duration_since(UNIX_EPOCH)
                  .expect("Can't get time for trip")
                  .as_secs()
              },
              |vehicle| vehicle.timestamp / 1000,
            )),
            delay: None,
          }),
          vehicle: None,
          alert: None,
        };
        if let Some(vehicle) = vehicle {
          let vehicle = FeedEntity {
            id: format!("vehicle-{}", vehicle.id),
            is_deleted: None,
            trip_update: None,
            vehicle: Some(VehiclePosition {
              trip: Some(arrival_data.trip_descriptor),
              vehicle: vehicle_descriptor,
              position: Some(Position {
                latitude: vehicle.position.0,
                longitude: vehicle.position.1,
                bearing: Some(vehicle.heading),
                odometer: None,
                speed: Some(mph_to_meters(vehicle.speed)),
              }),
              current_stop_sequence: Some(arrival_data.stop_time.stop_sequence),
              stop_id: Some(arrival_data.stop_time.stop_id.to_string()),
              current_status: Some(VehicleStopStatus::InTransitTo.into()),
              timestamp: Some(vehicle.timestamp / 1000),
              congestion_level: None,
              occupancy_status: None,
            }),
            alert: None,
          };
          Box::new(once(vehicle).chain(once(trip))) as Box<dyn Iterator<Item = FeedEntity>>
        } else {
          Box::new(once(trip)) as Box<dyn Iterator<Item = FeedEntity>>
        }
      })
      .unique_by(|entity| entity.id.clone())
      .filter(|entity| entity.trip_update.is_none())
      .collect(),
  )
}
