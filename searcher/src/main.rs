mod config;
mod filter;

use config::Config;
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
      if let Err(error) = handle_pubkeys(&marginfi, &accounts_batch, &filter).await {
        println!("Error fetching accounts: {}", error);
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

async fn handle_pubkeys<T>(protocol: &Marginfi, pubkeys: &[Pubkey], filter: &AccountFilter<T>) -> anyhow::Result<()>
  where I80F48: PartialOrd<T> {
  let start = Instant::now();
  let users = protocol.load_users(pubkeys).await?;
  let duration = start.elapsed();
  
  let mut hits = Vec::new();
  for result in users {
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
      let account = user.account();
      let bank_accounts = user.bank_accounts();
      let asset_value = user.asset_value()?;
      let liability_value = user.liability_value()?;
      let maint = user.maintenance()?;
      let maint_percentage = maint.checked_div(asset_value).unwrap_or(I80F48::from_num(1));
      
      println!("ACCOUNT: {}", account.authority);
      println!("  Lended assets ({}$):", asset_value);
      for bank_account in bank_accounts {
        let asset_shares: I80F48 = bank_account.balance.asset_shares.into();
        if asset_shares.is_zero() {
          continue;
        }
        println!("     Mint: {}", bank_account.bank.mint);
        println!("     Balance: {}", bank_account.bank.get_display_asset(bank_account.bank.get_asset_amount(asset_shares).unwrap()).unwrap());
      }
      println!("  Borrowed assets ({}$):", user.liability_value()?);
      for bank_account in bank_accounts {
        let liability_shares: I80F48 = bank_account.balance.liability_shares.into();
        if liability_shares.is_zero() {
          continue;
        }
        println!("     Mint: {}", bank_account.bank.mint);
        println!("     Balance: {}", bank_account.bank.get_display_asset(bank_account.bank.get_asset_amount(liability_shares).unwrap()).unwrap());
      }
      println!("  Maintenance: {}$ ({}%)", maint, maint_percentage.checked_mul_int(100).unwrap_or(I80F48::ZERO));

      hits.push(user);
    }
  }

  println!("LOADED {} USERS, {} HITS ({:?})", pubkeys.len(), hits.len(), duration);

  anyhow::Ok(())
}