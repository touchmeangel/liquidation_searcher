mod config;

use config::Config;
use connections::{PubRedis, SubRedis, queue_keys};
use protocols::marginfi::{Marginfi, MarginfiUser};
use solana_pubkey::Pubkey;
use std::sync::Arc;
use tokio::{signal, sync::{self, Semaphore}, time::Instant};

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
  let marginfi = Arc::new(Marginfi::new(config.http_url, config.ws_url).await?);
  let mut sub_redis = SubRedis::new(&config.pubsub_url).await?;
  let pub_redis = Arc::new(sync::Mutex::new(PubRedis::new(&config.pubsub_url).await?));
  println!("connection established, listening");

  let semaphore = Arc::new(Semaphore::new(config.capacity));

  loop {
    tokio::select! {
      result = sub_redis.builder::<()>(queue_keys::CHECK_QUEUE, config.accounts_batch_size).recv() => {
        let results = match result {
          Ok(messages) => messages,
          Err(err) => {
            println!("error while reading: {}", err);
            continue
          },
        };

        for result in &results {
          if let Err(err) = result {
            println!("failed to accept a message: {}", err);
          }
        }

        let accounts: Vec<_> = results.into_iter().filter_map(|result| result.map(|(pk, _)| pk).ok()).collect();
        
        if accounts.is_empty() {
          continue;
        }
        
        let permit = semaphore.clone();
        let marginfi_clone = Arc::clone(&marginfi);
        let pub_redis_clone = Arc::clone(&pub_redis);
        tokio::spawn(async move {
          let _guard =  permit.acquire().await.unwrap();

          if let Err(err) = handle(pub_redis_clone, &marginfi_clone, accounts).await {
            println!("error liquidating accounts: {}", err);
          };
        });
      }
      _ = signal::ctrl_c() => {
        println!("shutting down");
        break;
      }
    }
  }

  Ok(())
}

async fn handle(pub_redis_mutex: Arc<sync::Mutex<PubRedis>>, marginfi: &Marginfi, accounts: Vec<Pubkey>) -> anyhow::Result<()> {
  let start = Instant::now();
  let hits = check_pubkeys(marginfi, &accounts).await?;
  let duration = start.elapsed();
    
  println!("{} HITS OUT OF {} ({:?})", hits.len(), accounts.len(), duration);  
  let mut pub_redis = pub_redis_mutex.lock().await;
  let _ = pub_redis.builder::<MarginfiUser>(queue_keys::LIQUIDATION_QUEUE).items(hits.into_iter().map(|(pk, user)| (*pk, user))).send().await?;

  Ok(())
}

async fn check_pubkeys<'a>(protocol: &Marginfi, pubkeys: &'a [Pubkey]) -> anyhow::Result<Vec<(&'a Pubkey, MarginfiUser)>> {
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
    
    let result = match user.eligible_for_liquidation() {
      Ok(result) => result,
      Err(error) => {
        println!("Error: {}", error);
        continue;
      },
    };

    if result {
      hits.push((pubkey, user));
    }
  }

  anyhow::Ok(hits)
}