mod config;

use std::{marker::PhantomData, sync::Arc};

use config::Config;
use connections::{PubRedis, Redis, SubRedis, queue_keys};
use fixed::types::I80F48;
use protocols::marginfi::{AccountFilter, Marginfi, MarginfiUser};
use solana_pubkey::Pubkey;
use tokio::{signal, sync::Semaphore, time::Instant};

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
  let filter = Arc::new(AccountFilter {
    min_asset_value: config.min_asset_value,
    max_asset_value: config.max_asset_value,
    min_liability_value: config.min_liability_value,
    max_liability_value: config.max_liability_value,
    min_maint_percentage: config.min_maint_percentage,
    max_maint_percentage: config.max_maint_percentage,
    min_maint: config.min_maint,
    max_maint: config.max_maint,
  });

  let marginfi = Arc::new(Marginfi::new(config.http_url, config.ws_url).await?);
  let redis = Redis::new(&config.redis_url).await?;
  let pub_redis = PubRedis::new(&config.pubsub_url).await?;
  let mut sub_redis = SubRedis::new(&config.pubsub_url).await?;
  println!("connection established, listening");

  let semaphore = Arc::new(Semaphore::new(config.capacity));

  loop {
    tokio::select! {
      result = sub_redis.builder::<()>(queue_keys::ADD_QUEUE, config.accounts_batch_size).recv() => {
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
        let redis_clone = redis.clone();
        let pub_redis_clone = pub_redis.clone();
        let marginfi_clone = Arc::clone(&marginfi);
        let filter_clone = Arc::clone(&filter);
        tokio::spawn(async move {
          let _guard = permit.acquire().await.unwrap();

          if let Err(err) = handle(&marginfi_clone, redis_clone, pub_redis_clone, accounts, &filter_clone).await {
            println!("error removing accounts: {}", err);
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

async fn handle<T>(marginfi: &Marginfi, mut redis: Redis, mut pub_redis: PubRedis, accounts: Vec<Pubkey>, filter: &AccountFilter<T>) -> anyhow::Result<()>
  where I80F48: PartialOrd<T> {
  let mut accounts_that_dont_exist = Vec::new();
  for (exists, account) in redis.exists_multiple(&accounts).await?.into_iter().zip(accounts) {
    if !exists {
      accounts_that_dont_exist.push(account);
    }
  }

  let start = Instant::now();
  let items = check_pubkeys(marginfi, &accounts_that_dont_exist, filter).await?;
  let duration = start.elapsed();

  let len = items.len();
  if len == 0 {
    return Ok(());
  }

  let (users, pubkeys): (Vec<_>, Vec<_>) = items.into_iter().unzip();
  let bank_accounts: Vec<_> = users
    .iter()
    .map(|user| user.bank_accounts().iter().map(|bank_account| &bank_account.balance.bank_pk).collect::<Vec<_>>())
    .collect();
  let (new_banks, amount) = redis.add_multiple(pubkeys, bank_accounts).await?;

  if amount > 0 {
    println!("* added {} accounts ({:?})", amount, duration);
  }

  if new_banks.is_empty() {
    return Ok(())
  }

 let _ = pub_redis.builder::<()>(queue_keys::BANK_ADD_QUEUE).items(new_banks.into_iter().map(|bank| (bank, ()))).send().await?;

  Ok(())
}

async fn check_pubkeys<'a, T>(protocol: &Marginfi, pubkeys: &'a [Pubkey], filter: &AccountFilter<T>) -> anyhow::Result<Vec<(MarginfiUser, &'a Pubkey)>>
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

    if result {
      hits.push((user, pubkey));
    }
  }

  anyhow::Ok(hits)
}