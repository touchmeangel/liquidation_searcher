mod config;

use config::Config;
use connections::Redis;

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    let mut redis = Redis::new(&config.redis_url).await?;
    
    let accounts = redis.get_all().await?;
    println!("checking {} accounts", accounts.len());

    Ok(())
  }.await;

  if let Err(err) = result {
    eprintln!("error: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}