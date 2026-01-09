use std::collections::HashMap;

use anyhow::Context;
use fixed::types::I80F48;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use anchor_lang::prelude::{Clock, Pubkey};

use crate::{marginfi::types::{Bank, MarginfiAccount, OraclePriceFeedAdapter, OraclePriceFeedAdapterConfig, OraclePriceType, PriceAdapter}, utils::parse_account};

#[derive(Clone)]
pub struct MarginfiUserAccount {
  account: MarginfiAccount,
  banks: HashMap<Pubkey, BankWithPriceFeed>,
}

impl MarginfiUserAccount {
  pub async fn from_pubkey(rpc_client: &RpcClient, account_pubkey: &Pubkey, clock: &Clock) -> anyhow::Result<Self> {
    let account_data = rpc_client.get_account(account_pubkey).await?.data;
    let account = parse_account::<MarginfiAccount>(&account_data)
      .map_err(|e| anyhow::anyhow!("invalid account data: {}", e))?;
    
    let bank_pubkeys: Vec<Pubkey> = account
      .lending_account
      .get_active_balances_iter()
      .map(|balance| balance.bank_pk)
      .collect();

    let bank_accounts = rpc_client.get_multiple_accounts(&bank_pubkeys).await?
      .into_iter()
      .collect::<Option<Vec<_>>>()
      .ok_or(anyhow::anyhow!("get_multiple_accounts failed to load all bank accounts"))?;

    let banks = bank_accounts
      .iter()
      .map(|account| parse_account::<Bank>(&account.data))
      .collect::<Result<Vec<_>, _>>()
      .map_err(|e| anyhow::anyhow!("invalid bank data: {}", e))?;

    let configs = OraclePriceFeedAdapterConfig::load_multiple_with_clock(rpc_client, &banks, clock).await?;
    let price_feeds = configs
      .into_iter()
      .map(|cfg| OraclePriceFeedAdapter::try_from_config(cfg))
      .collect::<Result<Vec<_>, _>>()?;

    let banks = banks
      .into_iter()
      .zip(bank_pubkeys)
      .zip(price_feeds)
      .map(|((bank, bank_pk), price_feed)| (bank_pk, BankWithPriceFeed { bank, price_feed }))
      .collect();

    anyhow::Ok(Self {
      account,
      banks,
    })
  } 

  pub fn account(&self) -> &MarginfiAccount {
    &self.account
  }
  
  /// returns lending value in usd
  pub fn lending_value(&self) -> anyhow::Result<I80F48> {
    let total_asset_value: I80F48 = self.account.lending_account.get_active_balances_iter()
      .try_fold(I80F48::ZERO, |acc, balance| {
        let bank = self.banks.get(&balance.bank_pk)
          .ok_or_else(|| anyhow::anyhow!("Bank not found"))?;
    
        let price = bank.price_feed.get_price_of_type(
          OraclePriceType::RealTime,
          Some(super::types::PriceBias::Low),
          bank.bank.config.oracle_max_confidence
        )?;
    
        let asset = bank.bank.get_asset_amount(balance.asset_shares.into())
          .context("asset shares calculation failed")?;
    
        let asset_value = asset.checked_mul(price)
          .context("asset value calculation failed")?;
    
        anyhow::Ok(acc + asset_value)
      })?;

    anyhow::Ok(total_asset_value)
  }
}

#[derive(Clone)]
pub struct BankWithPriceFeed {
  pub bank: Bank,
  pub price_feed: OraclePriceFeedAdapter,
}