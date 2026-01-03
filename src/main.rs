mod config;
mod consts;

use config::Config;
use consts::MARGINFI_PROGRAM_ID;
use anchor_lang::prelude::*;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{CommitmentConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use tokio_stream::StreamExt;

#[event]
pub struct HealthPulseEvent {
  pub account: Pubkey,
  // pub health_cache: HealthCache,
}

#[tokio::main]
async fn main() {
  let result: anyhow::Result<()> = async move {
    let config = Config::open().await?;

    let pubsub = PubsubClient::new(config.ws_url).await?;
    
    let (mut logs, _unsub) = pubsub
        .logs_subscribe(
            RpcTransactionLogsFilter::Mentions(vec![MARGINFI_PROGRAM_ID.to_string()]),
            RpcTransactionLogsConfig {
              commitment: Some(CommitmentConfig::confirmed()),
            },
        )
        .await?;
    
    println!("✅ Connected! Listening for liquidation events...\n");
    
    while let Some(response) = logs.next().await {
      let signature = &response.value.signature;
      let err = response.value.err.is_some();
      
      if err {
        continue;
      }

      for log in &response.value.logs {
        if let Some(event_data) = log.strip_prefix("Program data: ") {
          if let Ok(event) = parse_anchor_event::<HealthPulseEvent>(event_data) {
            println!("HEALTH PULSE!");
            println!("  Account: {}", event.account);
            println!("  Transaction: {}", signature);
            println!();
          }
        }
      }
    }
    
    Ok(())
  }.await;

  if let Err(err) = result {
    eprintln!("error occurred during execution: {err}");
    
    err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}

fn parse_anchor_event<T: anchor_lang::AnchorDeserialize>(data: &str) -> anyhow::Result<T> {
  use base64::{Engine as _, engine::general_purpose};
  let decoded = general_purpose::STANDARD.decode(data)?;
  let event_data = &decoded[8..];
  Ok(T::deserialize(&mut &event_data[..])?)
}