// #![feature(future_join)]

mod alerts;
mod arrivals;
mod protobuf_route;
mod schedule;
mod traits;
use crate::protobuf_route::protobuf_route;

#[async_std::main]
async fn main() -> tide::Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
  let mut app = tide::new();
  app.with(tide::log::LogMiddleware::new());
  app.at("/rt/:agency_id/:agency_code").get(protobuf_route);
  let addr = "0.0.0.0:6969";
  println!("Ready to go at: http://{}", addr);
  app.listen(addr).await?;
  Ok(())
}
