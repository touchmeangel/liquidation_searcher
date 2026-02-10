mod config;

use std::sync::Arc;

use config::Config;
use connections::{SubRedis, queue_keys};
use protocols::marginfi::Marginfi;
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
  let marginfi = Arc::new(Marginfi::new(config.http_url, config.ws_url).await?);
  let mut subredis = SubRedis::new(&config.pubsub_url).await?;
  println!("connection established, listening");

  let semaphore = Arc::new(Semaphore::new(config.capacity));

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
        
        let permit = semaphore.clone();
        let marginfi_clone = Arc::clone(&marginfi);
        tokio::spawn(async move {
          let _guard = permit.acquire().await.unwrap();

          if let Err(err) = handle(&marginfi_clone, accounts).await {
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

async fn handle(marginfi: &Marginfi, accounts: Vec<Pubkey>) -> anyhow::Result<()> {
  println!("RECEIVED {} ACCOUNTS", accounts.len());

  Ok(())
}