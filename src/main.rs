mod rit_protobuf;
use crate::rit_protobuf::rit_protobuf;

#[async_std::main]
async fn main() -> tide::Result<()> {
  let mut app = tide::new();
  app.at("/rit.protobuf").get(rit_protobuf);
  let addr = "0.0.0.0:6969";
  println!("Ready to go at: {}", addr);
  app.listen(addr).await?;
  Ok(())
}
