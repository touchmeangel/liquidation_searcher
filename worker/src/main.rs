mod config;

use config::Config;
use connections::{SubRedis, queue_keys};
use tokio::signal;

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
  let mut subredis = SubRedis::new(&config.pubsub_url).await?;
  println!("Started listening...");

  loop {
    tokio::select! {
      result = subredis.read(queue_keys::CHECK_QUEUE, config.accounts_batch_size) => {
        let messages = match result {
          Ok(messages) => messages,
          Err(err) => {
            println!("error while reading: {}", err);
            continue
          },
        };
        
        if messages.is_empty() {
          continue;
        }
        
        println!("RECEIVED {} ACCOUNTS", messages.len());
      }
      _ = signal::ctrl_c() => {
        println!("shutting down");
        break;
      }
    }
  }

  Ok(())
}