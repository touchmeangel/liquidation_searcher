mod instructions;
mod user;
mod types;
mod consts;
mod errors;
mod events;
mod macros;
mod prelude;
mod wrapped_i80f48;

use fixed::types::I80F48;
use instructions::*;
use consts::*;
pub use errors::*;
use events::*;
use solana_rpc_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_transaction_status_client_types::option_serializer::OptionSerializer;
use wrapped_i80f48::*;
use user::*;

use std::collections::HashSet;
use std::rc::Rc;
use std::str::FromStr;

use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use solana_rpc_client_types::config::{RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use anchor_client::{Client, Cluster, Program};
use anchor_client::solana_sdk::signature::{Keypair, Signature};
use futures_util::stream::StreamExt;
use tokio_stream::iter;
use std::time::Instant;

use crate::consts::MARGINFI_PROGRAM_ID;

const TX_BATCH_SIZE: usize = 128;
const CONCURRENT_TX_REQUESTS: usize = 10;

pub struct Marginfi {
  pubsub: PubsubClient,
  rpc_client: RpcClient,
  client: Client<Rc<Keypair>>,
  program: Program<Rc<Keypair>>
}

impl Marginfi {
  pub async fn new(http_url: String, ws_url: String) -> anyhow::Result<Self> {
    let pubsub = PubsubClient::new(&ws_url).await?;
    let payer = Rc::new(Keypair::new());
    let client = Client::new(Cluster::Custom(http_url, ws_url), payer);
    let program = client.program(MARGINFI_PROGRAM_ID)?;
    let rpc_client = program.rpc();

    anyhow::Ok(Self { pubsub, rpc_client, client, program })
  }

  pub async fn scan_for_targets(&self) -> anyhow::Result<()> {
    let mut before_signature: Option<Signature> = None;

    println!("NEXT BATCH");
    loop {
      let sig_config = GetConfirmedSignaturesForAddress2Config {
        before: before_signature,
        until: None,
        limit: Some(TX_BATCH_SIZE),
        commitment: Some(CommitmentConfig::confirmed()),
      };
  
      let signatures = self.rpc_client.get_signatures_for_address_with_config(
        &MARGINFI_PROGRAM_ID,
        sig_config,
      ).await?;
  
      if signatures.is_empty() {
        break;
      }
      
      let results = iter(&signatures)
        .map(|sig_info| async {
          let signature = Signature::from_str(&sig_info.signature).ok()?;
          
          let config = RpcTransactionConfig {
            encoding: Some(solana_transaction_status_client_types::UiTransactionEncoding::Binary),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
          };
  
          println!("TX: {}", signature);
          
          match self.rpc_client.get_transaction_with_config(&signature, config).await {
            Ok(tx) => Some((signature, tx)),
            Err(e) => {
              eprintln!("Error fetching transaction {}: {}", signature, e);
              None
            }
          }
        })
        .buffer_unordered(CONCURRENT_TX_REQUESTS)
        .collect::<Vec<_>>()
        .await;
  
      for (signature, tx) in results.into_iter().flatten() {
        if let Some(meta) = tx.transaction.meta {
          if let OptionSerializer::Some(log_messages) = meta.log_messages {
            self.handle_logs(&log_messages).await;
          }
        }
        println!();
      }
  
      if signatures.len() < TX_BATCH_SIZE {
        break;
      }
      
      before_signature = Some(Signature::from_str(&signatures.last().unwrap().signature)?);
    }
    println!("NO TXs LEFT");

    anyhow::Ok(())
  }

  pub async fn listen_for_targets(&self) -> anyhow::Result<()> {
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

      println!("TX: {}", signature);
      self.handle_logs(&response.value.logs).await;
      println!();
    }

    anyhow::Ok(())
  }

  async fn handle_logs(&self, logs: &[String]) {
    let mut marginfi_accounts = Vec::new();

    for log in logs {
      if let Some(event_data) = log.strip_prefix("Program data: ") {
        if let Ok(event) = parse_anchor_event::<LendingAccountDepositEvent>(event_data) {
          println!("  DEPOSIT!");
          marginfi_accounts.push(event.header.marginfi_account);
        }
        if let Ok(event) = parse_anchor_event::<LendingAccountBorrowEvent>(event_data) {
          println!("  BORROW!");
          marginfi_accounts.push(event.header.marginfi_account);
        }
        if let Ok(event) = parse_anchor_event::<LendingAccountRepayEvent>(event_data) {
          println!("  REPAY!");
          marginfi_accounts.push(event.header.marginfi_account);
        }
        if let Ok(event) = parse_anchor_event::<LendingAccountWithdrawEvent>(event_data) {
          println!("  WITHDRAW!");
          marginfi_accounts.push(event.header.marginfi_account);
        }
      }
    }

    let mut seen = HashSet::new();
    for account in marginfi_accounts.into_iter().filter(|x| seen.insert(*x)) {
      if let Err(error) = self.handle_account(&account).await {
        println!("Error, skipping: {}", error)
      }
    }
  }

  async fn handle_account(&self, account_pubkey: &anchor_lang::prelude::Pubkey) -> anyhow::Result<()> {
    let start = Instant::now();
    let account = MarginfiUserAccount::from_pubkey(&self.rpc_client, account_pubkey).await?;
    let marginfi_account = account.account();
    let bank_accounts = account.bank_accounts();
    let duration = start.elapsed();
    println!("ACCOUNT DATA ({:?})", duration);
    println!("  Owner: {}", marginfi_account.authority);
    let asset_value = account.asset_value()?;
    println!("  Lended assets ({}$):", asset_value);
    for bank_account in bank_accounts {
      let asset_shares: I80F48 = bank_account.balance.asset_shares.into();
      if asset_shares.is_zero() {
        continue;
      }
      println!("     Mint: {}", bank_account.bank.mint);
      println!("     Balance: {}", bank_account.bank.get_display_asset(bank_account.bank.get_asset_amount(asset_shares).unwrap()).unwrap());
    }
    println!("  Borrowed assets ({}$):", account.liability_value()?);
    for bank_account in bank_accounts {
      let liability_shares: I80F48 = bank_account.balance.liability_shares.into();
      if liability_shares.is_zero() {
        continue;
      }
      println!("     Mint: {}", bank_account.bank.mint);
      println!("     Balance: {}", bank_account.bank.get_display_asset(bank_account.bank.get_asset_amount(liability_shares).unwrap()).unwrap());
    }
    let maint = account.maintenance()?;
    println!("  Maintenance: {}$ ({}%)", maint, maint.checked_div(asset_value).unwrap_or(I80F48::from_num(1)).checked_mul_int(100).unwrap());

    anyhow::Ok(())
  }
}

fn parse_anchor_event<T: anchor_lang::AnchorDeserialize>(data: &str) -> anyhow::Result<T> {
  use base64::{Engine as _, engine::general_purpose};
  let decoded = general_purpose::STANDARD.decode(data)?;
  let event_data = &decoded[8..];
  Ok(T::deserialize(&mut &event_data[..])?)
}