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

    sleep(Duration::from_secs(3600)).await;  
  }
}

async fn search(config: Config) -> anyhow::Result<()> {  
  let mut pub_redis = PubRedis::new(&config.pubsub_url).await?;

  let marginfi = Marginfi::new(config.http_url, config.ws_url).await?;

  let start = Instant::now();
  let accounts = marginfi.get_all_accounts().await?;

  let duration = start.elapsed();
  println!("* found {} marginfi accounts ({:?})", accounts.len(), duration);

  let accounts_len = accounts.len();
  let result = pub_redis.builder::<()>(queue_keys::ADD_QUEUE).items(accounts.into_iter().map(|bank| (bank, ()))).send().await?;
  println!("* uploaded {} accounts out of {}", result.len(), accounts_len);
  
  Ok(())
}