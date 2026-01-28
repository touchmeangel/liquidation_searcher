mod config;

use config::Config;

use protocols::marginfi::{AccountFilter, Marginfi};

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    let marginfi: Marginfi<_> = Marginfi::new(config.url, config.ws_url, Some(AccountFilter {
      min_asset_value: Some(10.0),
      max_asset_value: None,
      min_maint_percentage: None,
      max_maint_percentage: Some(0.2),
      min_maint: None,
      max_maint: None
    })).await?;
    marginfi.look_for_targets().await?;
    
    Ok(())
  }.await;

  if let Err(err) = result {
    eprintln!("Error: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}