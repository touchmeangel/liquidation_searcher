mod config;

use std::time::Duration;

use config::Config;
use connections::{PubRedis, Redis, queue_keys};
use tokio::{signal, time};

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

  loop {
    tokio::select! {
      _ = broadcast(&mut redis, &mut pubredis) => {
        time::sleep(Duration::from_secs(1800)).await; 
      }
      _ = signal::ctrl_c() => {
        println!("shutting down...");
        break;
      }
    }
  }

  Ok(())
}

async fn broadcast(redis: &mut Redis, pubredis: &mut PubRedis) -> anyhow::Result<()> {
  let accounts = redis.get_all().await?;
  if accounts.is_empty() {
    println!("* no accounts synced");
    return Ok(());
  }

  let ids = pubredis.publish(queue_keys::REM_QUEUE, &accounts).await?;

  println!("* synced {} accounts out of {}", ids.len(), accounts.len());

  Ok(())
}