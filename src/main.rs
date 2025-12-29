use crate::config::Config;

mod config;

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    println!("rpc: {}", config.rpc_url);

    Ok(())
  }.await;

  if let Err(err) = result {
    eprintln!("error occurred during execution: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}
