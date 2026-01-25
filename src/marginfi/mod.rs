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
use wrapped_i80f48::*;
use user::*;

use std::collections::HashSet;
use std::rc::Rc;

use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use solana_rpc_client_types::config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use anchor_client::{Client, Cluster, Program};
use anchor_client::solana_sdk::signature::Keypair;
use tokio_stream::StreamExt;
use std::time::Instant;

use crate::consts::MARGINFI_PROGRAM_ID;

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

      let mut marginfi_accounts = Vec::new();

      println!("TX: {}", signature);
      for log in &response.value.logs {
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
      for account in marginfi_accounts.into_iter().filter(|x| seen.insert(x.clone())) {
        if let Err(error) = self.handle_account(&account).await {
          println!("Error, skipping: {}", error)
        }
      }
      println!();
    }

    anyhow::Ok(())
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