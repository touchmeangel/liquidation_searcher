mod config;
mod consts;
mod marginfi;
mod utils;

use config::Config;
use fixed::types::I80F48;

use crate::marginfi::{AccountFilter, Marginfi};

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    let marginfi = Marginfi::new(config.url, config.ws_url, Some(AccountFilter {
      min_asset_value: Some(I80F48::from_num(10)),
      max_asset_value: None,
      min_maint_percentage: None,
      max_maint_percentage: Some(I80F48::from_num(0.2)),
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