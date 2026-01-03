mod config;
mod consts;
mod marginfi;

use config::Config;

use crate::marginfi::Marginfi;

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    let marginfi = Marginfi::new(config.ws_url).await?;
    marginfi.listen().await?;
    
    Ok(())
  }.await;

  if let Err(err) = result {
    eprintln!("error occurred during execution: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}