mod config;

use config::Config;
use connections::{Redis, SubRedis, queue_keys};
use fixed::types::I80F48;
use protocols::marginfi::{AccountFilter, Marginfi};
use solana_pubkey::Pubkey;
use tokio::{signal, time::Instant};

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
  let filter = AccountFilter {
    min_asset_value: Some(10.0),
    max_asset_value: None,
    min_maint_percentage: None,
    max_maint_percentage: Some(0.2),
    min_maint: None,
    max_maint: None
  };

  let marginfi = Marginfi::new(config.http_url, config.ws_url).await?;
  let mut redis = Redis::new(&config.redis_url).await?;
  let mut subredis = SubRedis::new(&config.pubsub_url).await?;
  println!("connection established, listening");

  loop {
    tokio::select! {
      result = subredis.read(queue_keys::ADD_QUEUE, config.accounts_batch_size) => {
        let accounts = match result {
          Ok(messages) => messages,
          Err(err) => {
            println!("error while reading: {}", err);
            continue
          },
        };
        
        if accounts.is_empty() {
          continue;
        }
        
        if let Err(err) = handle(&marginfi, &mut redis, accounts, &filter).await {
          println!("error adding accounts: {}", err);
        };
      }
      _ = signal::ctrl_c() => {
        println!("shutting down");
        break;
      }
    }
  }

  Ok(())
}

async fn handle<T>(marginfi: &Marginfi, redis: &mut Redis, accounts: Vec<Pubkey>, filter: &AccountFilter<T>) -> anyhow::Result<()>
  where I80F48: PartialOrd<T> {
  let start = Instant::now();
  let items = check_pubkeys(marginfi, &accounts, filter).await?;
  let duration = start.elapsed();

  let len = items.len();
  if len == 0 {
    return Ok(());
  }

  let result = redis.rem(items).await?;

  if result > 0 {
    println!("* removed {} accounts ({:?})", result, duration);
  }

  Ok(())
}

async fn check_pubkeys<'a, T>(protocol: &Marginfi, pubkeys: &'a [Pubkey], filter: &AccountFilter<T>) -> anyhow::Result<Vec<&'a Pubkey>>
  where I80F48: PartialOrd<T> {
  let users = protocol.load_users(pubkeys).await?;
  
  let mut hits = Vec::new();
  for (result, pubkey) in users.into_iter().zip(pubkeys) {
    let user = match result {
      Ok(user) => user,
      Err(error) => {
        // println!("Error, skipping: {}", error);
        continue;   
      },
    };

    let result = match filter.check(&user) {
      Ok(result) => result,
      Err(error) => {
        println!("Error: {}", error);
        continue;
      },
    };

    if !result {
      hits.push(pubkey);
    }
  }

  anyhow::Ok(hits)
}