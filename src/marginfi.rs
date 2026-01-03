use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{CommitmentConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use tokio_stream::StreamExt;
use anchor_lang::prelude::*;

use crate::consts::MARGINFI_PROGRAM_ID;

#[event]
pub struct HealthPulseEvent {
  pub account: Pubkey,
  // pub health_cache: HealthCache,
}

pub struct Marginfi {
  pubsub: PubsubClient
}

impl Marginfi {
  pub async fn new(ws_url: String) -> anyhow::Result<Self> {
    let pubsub = PubsubClient::new(ws_url).await?;
    
    anyhow::Ok(Self { pubsub })
  }

  pub async fn listen(&self) -> anyhow::Result<()> {
    let (mut logs, _unsub) = self.pubsub
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

    anyhow::Ok(())
  }
}

fn parse_anchor_event<T: anchor_lang::AnchorDeserialize>(data: &str) -> anyhow::Result<T> {
  use base64::{Engine as _, engine::general_purpose};
  let decoded = general_purpose::STANDARD.decode(data)?;
  let event_data = &decoded[8..];
  Ok(T::deserialize(&mut &event_data[..])?)
}