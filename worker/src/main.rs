mod config;

use std::sync::Arc;

use config::Config;
use connections::{SubRedis, queue_keys};
use fixed::types::I80F48;
use protocols::marginfi::{FeeState, Marginfi, MarginfiUser};
use solana_pubkey::Pubkey;
use tokio::{signal, sync::Semaphore};

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
  let marginfi = Arc::new(Marginfi::new(config.http_url.clone(), config.ws_url.clone()).await?);
  let fee_state = Arc::new(marginfi.get_fee_state().await?);
  let liquidation_max_fee: I80F48 = fee_state.liquidation_max_fee.into();
  let liquidation_flat_sol_fee: I80F48 = fee_state.liquidation_flat_sol_fee.into();
  println!("FeeState is currently defined as liquidation_max_fee = {}% liquidation_flat_sol_fee = {} SOL", liquidation_max_fee.checked_mul(I80F48::from_num(100)).unwrap_or(I80F48::ZERO), liquidation_flat_sol_fee.checked_div(I80F48::from_num(1_000_000_000)).unwrap_or(I80F48::ZERO));

  let mut subredis = SubRedis::new(&config.pubsub_url).await?;
  println!("connection established, listening");

  let semaphore = Arc::new(Semaphore::new(config.capacity));

  loop {
    tokio::select! {
      result = subredis.builder::<MarginfiUser>(queue_keys::LIQUIDATION_QUEUE, 1).recv() => {
        let mut accounts = match result {
          Ok(messages) => messages,
          Err(err) => {
            println!("error while reading: {}", err);
            continue
          },
        };
        
        let result = match accounts.pop() {
          Some(account) => account,
          None => continue,
        };
        let (pubkey, account) = match result {
          Ok((pubkey, account)) => (pubkey, account),
          Err(err) => {
            println!("error parsing arguments: {}", err);
            continue
          },
        };
        
        let permit = semaphore.clone();
        let config_clone = config.clone();
        let marginfi_clone = Arc::clone(&marginfi);
        let fee_state_clone = Arc::clone(&fee_state);
        tokio::spawn(async move {
          let _guard = permit.acquire().await.unwrap();

          if let Err(err) = handle(config_clone, &marginfi_clone, &fee_state_clone, pubkey, account).await {
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

async fn handle(config: Config, marginfi: &Marginfi, fee_state: &FeeState, pubkey: Pubkey, account: MarginfiUser) -> anyhow::Result<()> {
  println!("RECEIVED {}", pubkey);
  let withdrawable_assets = account.withdrawable_asset_value()?;
  let liability = account.liability_value()?;

  let seizable = withdrawable_assets.checked_sub(liability).ok_or(anyhow::anyhow!("Math error at {}", line!()))?;
  if seizable <= 0 {
    println!("{} is deep in debt, not profitable to liquidate", pubkey);
    return anyhow::Ok(());
  }
  
  println!("{}$ to make, max {}$ (w: {}, l: {})", seizable, liability.checked_mul(fee_state.liquidation_max_fee.into()).unwrap_or(I80F48::ZERO), withdrawable_assets, liability);
  let mut tokens_to_swap = Vec::new();
  let max_assets = liability
    + liability
      .checked_mul(fee_state.liquidation_max_fee.into())
      .ok_or(anyhow::anyhow!("Math error at {}", line!()))?;
  let haircut = I80F48::from_num(config.asset_haircut);

  let assets_needed = max_assets
    .checked_mul(haircut)
    .ok_or(anyhow::anyhow!("Math error at {}", line!()))?;

  let mut assets_left = assets_needed;
  let mut banks_with_value: Vec<_> = account
    .bank_accounts()
    .iter()
    .map(|bank| -> anyhow::Result<_> {
      let value = bank.asset_value()?;
      Ok((bank, value))
    })
    .collect::<anyhow::Result<Vec<_>>>()?;

  banks_with_value.sort_unstable_by(|(_, a), (_, b)| {
    b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
  });
  
  for (bank, asset_value) in banks_with_value {
    if asset_value <= I80F48::ZERO {
      continue;
    }

    if asset_value > assets_left {
      let ratio = assets_left
        .checked_div(asset_value)
        .ok_or(anyhow::anyhow!("Math error at {}", line!()))?;
      let asset_shares: I80F48 = bank.balance.asset_shares.into();
      let shares_to_add = asset_shares
        .checked_mul(ratio)
        .ok_or(anyhow::anyhow!("Math error at {}", line!()))?;

      tokens_to_swap.push((bank.bank.mint, shares_to_add));
      break;
    }

    tokens_to_swap.push((bank.bank.mint, bank.balance.asset_shares.into()));
    assets_left = assets_left
      .checked_sub(asset_value)
      .ok_or(anyhow::anyhow!("Math error at {}", line!()))?;

    if assets_left == I80F48::ZERO {
      break;
    }
  }



  // 3VzSmqcYQaKcA8vFoqW5batNPNWVvqpVXtFmKHse7SUE
  // AiC3orMdwW2hG9Xhv53nktgDwq4cLkqLAfMcNQFoXWoJ
  // 2qD4c8Z4kFM8s629igaw9Rbc2DGx67bS2w2VawVAwaLd
  // 3GsZWEBFuoe1ooX8PiFASQPnfCeDZNdY8cCFB8ZESHfT
  // GnSNK7gpepE1PRZFSYCNr1KrBZXf8mBMiZzjn54forzw

  // DKTZBDzCgFcHu8QhjQCupo2GRiKbDWLmAYxwVUn38S5J
  // LENDING:
  // JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN: 47.69020247742894$
  // 27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4: 9.883730130318696$
  // BORROWING:
  // susdabGDNbhrnCa6ncrYo81u4s9GM8ecK2UwMyZiq4X: 51.69141136818984$

  Ok(())
}

// async fn liquidate()