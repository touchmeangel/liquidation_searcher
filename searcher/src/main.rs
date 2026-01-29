mod config;
mod filter;

use config::Config;
use connections::Redis;
use filter::AccountFilter;

use fixed::types::I80F48;
use protocols::marginfi::Marginfi;
use solana_pubkey::Pubkey;
use tokio::time::Instant;

const ACCOUNTS_BATCH: usize = 1000;

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    let mut redis = Redis::new(&config.redis_url).await?;

    let filter = AccountFilter {
      min_asset_value: Some(10.0),
      max_asset_value: None,
      min_maint_percentage: None,
      max_maint_percentage: Some(0.2),
      min_maint: None,
      max_maint: None
    };
    let marginfi = Marginfi::new(config.http_url, config.ws_url).await?;

    let start = Instant::now();
    let mut accounts = marginfi.get_all_accounts().await?;

    let duration = start.elapsed();
    println!("* found {} marginfi accounts ({:?})", accounts.len(), duration);

    let mut batches: Vec<Vec<_>> = Vec::new();
    while !accounts.is_empty() {
      let take = accounts.drain(..ACCOUNTS_BATCH.min(accounts.len())).collect();
      batches.push(take);
    }

    for accounts_batch in batches {
      let start = Instant::now();
      let items = match check_pubkeys(&marginfi, &accounts_batch, &filter).await {
        Ok(items) => items,
        Err(error) => {
          println!("error fetching accounts: {}", error);
          continue;
        },
      };
      let duration = start.elapsed();
      
      let len = items.len();
      if len == 0 {
        continue;
      }

      let result = match redis.add(items).await {
        Ok(result) => result,
        Err(error) => {
          println!("* error adding {} accounts: {}", len, error);
          continue;
        },
      };

      if result > 0 {
        println!("* added {} accounts ({:?})", result, duration);
      }
    }
    
    Ok(())
  }.await;

  if let Err(err) = result {
    eprintln!("Error: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
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

    if result {
      hits.push(pubkey);
    }
  }

  anyhow::Ok(hits)
}