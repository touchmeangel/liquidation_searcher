mod config;

use config::Config;
use connections::{PubRedis, queue_keys};

use protocols::marginfi::Marginfi;
use tokio::time::{Instant, sleep, Duration};

#[tokio::main]
async fn main() {
  let config = Config::open().unwrap();

  loop {
    let result = search(config.clone()).await;
  
    if let Err(err) = result {
      eprintln!("error: {err}");
      
      err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
    }

    sleep(Duration::from_secs(18000)).await;  
  }
}

async fn search(config: Config) -> anyhow::Result<()> {  
  let mut pubredis = PubRedis::new(&config.pubsub_url).await?;

  let marginfi = Marginfi::new(config.http_url, config.ws_url).await?;

  let start = Instant::now();
  let accounts = marginfi.get_all_accounts().await?;

  let duration = start.elapsed();
  println!("* found {} marginfi accounts ({:?})", accounts.len(), duration);

  let result = pubredis.publish(queue_keys::ADD_QUEUE, &accounts).await?.len();
  println!("* published {} accounts out of {}", accounts.len(), result);
  
  Ok(())
}