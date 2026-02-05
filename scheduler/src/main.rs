mod config;

use config::Config;
use connections::{PubRedis, Redis};

#[tokio::main]
async fn main() {
  let config = Config::open().unwrap();

  let result = start(config).await;

  if let Err(err) = result {
    eprintln!("error: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}

async fn start(config: Config) -> anyhow::Result<()> {
  let mut redis = Redis::new(&config.redis_url).await?;
  let mut pubredis = PubRedis::new(&config.pubsub_url).await?;

  broadcast(&mut redis, &mut pubredis).await
}

async fn broadcast(redis: &mut Redis, pubredis: &mut PubRedis) -> anyhow::Result<()> {
  let accounts = redis.get_all().await?;
  let ids = pubredis.publish(&accounts).await?;

  println!("* published {} accounts out of {}", ids.len(), accounts.len());

  Ok(())
}