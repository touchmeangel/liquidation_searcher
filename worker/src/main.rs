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

  Ok(())
}