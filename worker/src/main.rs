mod config;

use config::Config;
use connections::Redis;
use protocols::marginfi::Marginfi;

#[tokio::main]
async fn main() {
  let config = Config::open().unwrap();

  let result = liquidate(config).await;

  if let Err(err) = result {
    eprintln!("error: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}

async fn liquidate(config: Config) -> anyhow::Result<()> {
  let mut redis = Redis::new(&config.redis_url).await?;
  let marginfi = Marginfi::new(config.http_url, config.ws_url).await?;
  
  let mut accounts = redis.get_all().await?;
  let mut batches: Vec<Vec<_>> = Vec::new();
  while !accounts.is_empty() {
    let take = accounts.drain(..config.accounts_batch_size.min(accounts.len())).collect();
    batches.push(take);
  }

  for accounts_batch in batches {
    let results = marginfi.load_users(&accounts_batch).await?;
    for result in results {

    }

    println!("checking {} accounts", accounts.len());
  }

  Ok(())
}