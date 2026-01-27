mod instructions;
mod user;
mod types;
mod consts;
mod errors;
mod events;
mod macros;
mod prelude;
mod wrapped_i80f48;

use anchor_lang::Discriminator;
use fixed::types::I80F48;
use instructions::*;
use consts::*;
pub use errors::*;
use events::*;
use solana_account_decoder::UiDataSliceConfig;
use solana_rpc_client_types::filter::{Memcmp, RpcFilterType};
use wrapped_i80f48::*;
use user::*;

use std::collections::HashSet;
use std::rc::Rc;

use anchor_lang::prelude::Pubkey;
use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use solana_rpc_client_types::config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use anchor_client::{Client, Cluster, Program};
use anchor_client::solana_sdk::signature::Keypair;
use futures_util::stream::StreamExt;
use tokio::time::Instant;

use crate::consts::MARGINFI_PROGRAM_ID;
use crate::marginfi::types::MarginfiAccount;

const ACCOUNTS_BATCH: usize = 1000;

#[derive(Debug, Clone)]
pub struct AccountFilter {
  pub min_asset_value: Option<I80F48>,
  pub max_asset_value: Option<I80F48>,
  pub min_maint_percentage: Option<I80F48>,
  pub max_maint_percentage: Option<I80F48>,
  pub min_maint: Option<I80F48>,
  pub max_maint: Option<I80F48>,
}

impl Default for AccountFilter {
  fn default() -> Self {
    Self {
      min_asset_value: None,
      max_asset_value: None,
      min_maint_percentage: None,
      max_maint_percentage: None,
      min_maint: None,
      max_maint: None,
    }
  }
}

pub struct Marginfi {
  pubsub: PubsubClient,
  rpc_client: RpcClient,
  client: Client<Rc<Keypair>>,
  program: Program<Rc<Keypair>>,
  filter: AccountFilter
}

impl Marginfi {
  pub async fn new(http_url: String, ws_url: String, account_filter: Option<AccountFilter>) -> anyhow::Result<Self> {
    let pubsub = PubsubClient::new(&ws_url).await?;
    let payer = Rc::new(Keypair::new());
    let client = Client::new(Cluster::Custom(http_url, ws_url), payer);
    let program = client.program(MARGINFI_PROGRAM_ID)?;
    let rpc_client = program.rpc();

    anyhow::Ok(Self { pubsub, rpc_client, client, program, filter: account_filter.unwrap_or_default() })
  }

  pub async fn look_for_targets(&self) -> anyhow::Result<()> {
    let filters = vec![
      RpcFilterType::Memcmp(Memcmp::new(
        0,
        solana_rpc_client_types::filter::MemcmpEncodedBytes::Bytes(Vec::from(MarginfiAccount::DISCRIMINATOR))
      )),
    ];

    let config = RpcProgramAccountsConfig {
      filters: Some(filters),
      account_config: RpcAccountInfoConfig {
        encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
        data_slice: Some(UiDataSliceConfig {
          offset: 0,
          length: 0,
        }),
        commitment: Some(CommitmentConfig::confirmed()),
        min_context_slot: None,
      },
      with_context: None,
      sort_results: None,
    };

    let start = Instant::now();
    let mut accounts = self.rpc_client
      .get_program_accounts_with_config(&MARGINFI_PROGRAM_ID, config)
      .await?;
  
    let duration = start.elapsed();
    println!("Found {} marginfi accounts ({:?})", accounts.len(), duration);

    let mut batches: Vec<Vec<_>> = Vec::new();

    while !accounts.is_empty() {
      let take = accounts.drain(..ACCOUNTS_BATCH.min(accounts.len())).collect();
      batches.push(take);
    }

    for accounts_batch in batches {
      let (pubkeys, _): (Vec<_>, Vec<_>) = accounts_batch.into_iter().unzip();
      if let Err(error) = self.handle_pubkeys(&pubkeys).await {
        println!("Error fetching accounts: {}", error);
      }
    }

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
      let marginfi_accounts = self.parse_logs(&response.value.logs);
      let mut seen = HashSet::new();
      let unique: Vec<_> = marginfi_accounts.into_iter().filter(|x| seen.insert(*x)).collect();
      drop(seen);
      if let Err(error) = self.handle_pubkeys(&unique).await {
        println!("Error fetching accounts: {}", error);
      }
      println!();
    }

    anyhow::Ok(())
  }

  fn parse_logs(&self, logs: &[String]) -> Vec<anchor_lang::prelude::Pubkey> {
    let mut marginfi_accounts = Vec::new();

    for log in logs {
      if let Some(event_data) = log.strip_prefix("Program data: ") {
        if let Ok(event) = parse_anchor_event::<LendingAccountDepositEvent>(event_data) {
          marginfi_accounts.push(event.header.marginfi_account);
        }
        if let Ok(event) = parse_anchor_event::<LendingAccountBorrowEvent>(event_data) {
          marginfi_accounts.push(event.header.marginfi_account);
        }
        if let Ok(event) = parse_anchor_event::<LendingAccountRepayEvent>(event_data) {
          marginfi_accounts.push(event.header.marginfi_account);
        }
        if let Ok(event) = parse_anchor_event::<LendingAccountWithdrawEvent>(event_data) {
          marginfi_accounts.push(event.header.marginfi_account);
        }
      }
    }

    marginfi_accounts
  }

  async fn handle_pubkeys(&self, pubkeys: &[Pubkey]) -> anyhow::Result<()> {
    let start = Instant::now();
    let marginfi_accounts = MarginfiUserAccount::from_pubkeys(&self.rpc_client, pubkeys).await?;
    let duration = start.elapsed();
    let mut hits = Vec::new();
    for result in marginfi_accounts {
      let marginfi_account = match result {
        Ok(marginfi_account) => marginfi_account,
        Err(error) => {
          // println!("Error, skipping: {}", error);
          continue;   
        },
      };

      let result = match self.check_account(&marginfi_account) {
        Ok(result) => result,
        Err(error) => {
          println!("Error: {}", error);
          continue;
        },
      };

      if result {
        let account = marginfi_account.account();
        let bank_accounts = marginfi_account.bank_accounts();
        let asset_value = marginfi_account.asset_value()?;
        let liability_value = marginfi_account.liability_value()?;
        let maint = marginfi_account.maintenance()?;
        let maint_percentage = maint.checked_div(asset_value).unwrap_or(I80F48::from_num(1));
        
        println!("ACCOUNT: {}", account.authority);
        println!("  Lended assets ({}$):", asset_value);
        for bank_account in bank_accounts {
          let asset_shares: I80F48 = bank_account.balance.asset_shares.into();
          if asset_shares.is_zero() {
            continue;
          }
          println!("     Mint: {}", bank_account.bank.mint);
          println!("     Balance: {}", bank_account.bank.get_display_asset(bank_account.bank.get_asset_amount(asset_shares).unwrap()).unwrap());
        }
        println!("  Borrowed assets ({}$):", marginfi_account.liability_value()?);
        for bank_account in bank_accounts {
          let liability_shares: I80F48 = bank_account.balance.liability_shares.into();
          if liability_shares.is_zero() {
            continue;
          }
          println!("     Mint: {}", bank_account.bank.mint);
          println!("     Balance: {}", bank_account.bank.get_display_asset(bank_account.bank.get_asset_amount(liability_shares).unwrap()).unwrap());
        }
        println!("  Maintenance: {}$ ({}%)", maint, maint_percentage.checked_mul_int(100).unwrap_or(I80F48::ZERO));

        hits.push(marginfi_account);
      }
    }

    println!("CHECKED {} ACCOUNTS, {} HITS ({:?})", pubkeys.len(), hits.len(), duration);

    anyhow::Ok(())
  }

  // Seized <= Repaid * (1 + max_fee)
  // Where Seized is the equity value withdrawn, in Repaidistheequityvaluerepaid, in,
  // and max fee is the maximum allowed profit currently configured at 10%.
  // Note that equity value is the price of the token without any weights applied, but inclusive of oracle confidence interval adjustments.
  fn check_account(&self, account: &MarginfiUserAccount) -> anyhow::Result<bool> {
    let bank_accounts = account.bank_accounts();
    let asset_value = account.asset_value()?;
    let liability_value = account.liability_value()?;
    let maint = account.maintenance()?;
    let maint_percentage = maint.checked_div(asset_value).unwrap_or(I80F48::from_num(1));
    
    if let Some(min) = self.filter.min_asset_value {
      if asset_value < min {
        return Ok(false);
      }
    }
    
    if let Some(max) = self.filter.max_asset_value {
      if asset_value > max {
        return Ok(false);
      }
    }
    
    if let Some(min) = self.filter.min_maint_percentage {
      if maint_percentage < min {
        return Ok(false);
      }
    }
    
    if let Some(max) = self.filter.max_maint_percentage {
      if maint_percentage >= max {
        return Ok(false);
      }
    }
    
    if let Some(min) = self.filter.min_maint {
      if maint < min {
        return Ok(false);
      }
    }
    
    if let Some(max) = self.filter.max_maint {
      if maint > max {
        return Ok(false);
      }
    }
    
    Ok(true)
  }
}

fn parse_anchor_event<T: anchor_lang::AnchorDeserialize>(data: &str) -> anyhow::Result<T> {
  use base64::{Engine as _, engine::general_purpose};
  let decoded = general_purpose::STANDARD.decode(data)?;
  let event_data = &decoded[8..];
  Ok(T::deserialize(&mut &event_data[..])?)
}