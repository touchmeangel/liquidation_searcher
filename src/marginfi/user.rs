use anyhow::Context;
use fixed::types::I80F48;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use anchor_lang::prelude::Pubkey;

use crate::{marginfi::types::{Balance, BalanceSide, Bank, EmodeConfig, MarginfiAccount, OraclePriceFeedAdapter, OraclePriceFeedAdapterConfig, OraclePriceType, PriceAdapter, reconcile_emode_configs}, utils::parse_account};

#[derive(Clone)]
pub struct MarginfiUserAccount {
  account: MarginfiAccount,
  bank_accounts: Vec<BankAccount>,
  emode_config: EmodeConfig
}

impl MarginfiUserAccount {
  pub async fn from_pubkeys(
    rpc_client: &RpcClient, 
    account_pubkeys: &[Pubkey]
  ) -> anyhow::Result<Vec<anyhow::Result<Self>>> {
    if account_pubkeys.is_empty() {
      return Ok(Vec::new());
    }
  
    let marginfi_accounts_data = rpc_client
      .get_multiple_accounts(account_pubkeys)
      .await?;
  
    let marginfi_accounts: Vec<Option<MarginfiAccount>> = marginfi_accounts_data
      .into_iter()
      .map(|opt_account| {
        opt_account.and_then(|account| {
          parse_account::<MarginfiAccount>(&account.data).ok()
        })
      })
      .collect();
  
    let mut all_bank_pubkeys: Vec<Pubkey> = marginfi_accounts
      .iter()
      .flatten()
      .flat_map(|account| {
        account
          .lending_account
          .get_active_balances_iter()
          .map(|balance| balance.bank_pk)
      })
      .collect();
  
    all_bank_pubkeys.sort();
    all_bank_pubkeys.dedup();
  
    let bank_accounts_data = rpc_client
      .get_multiple_accounts(&all_bank_pubkeys)
      .await?;
  
    let all_banks: Vec<Option<Bank>> = bank_accounts_data
      .into_iter()
      .map(|opt_account| {
        opt_account.and_then(|account| {
          parse_account::<Bank>(&account.data).ok()
        })
      })
      .collect();
  
    let banks_map: std::collections::HashMap<Pubkey, Bank> = all_bank_pubkeys
      .iter()
      .zip(all_banks.iter())
      .filter_map(|(pk, opt_bank)| {
        opt_bank.as_ref().map(|bank| (*pk, *bank))
      })
      .collect();
  
    let successfully_loaded_banks: Vec<Bank> = all_banks
      .into_iter()
      .flatten()
      .collect();
  
    let all_configs_result = OraclePriceFeedAdapterConfig::load_multiple(
      rpc_client, 
      &successfully_loaded_banks
    ).await;
  
    let price_feeds_map: std::collections::HashMap<Pubkey, anyhow::Result<OraclePriceFeedAdapter>> = 
      match all_configs_result {
        Ok(configs) => {
          all_bank_pubkeys
            .iter()
            .zip(configs.into_iter())
            .map(|(pk, cfg)| {
              let price_feed_result = OraclePriceFeedAdapter::try_from_config(cfg).map_err(|err| anyhow::anyhow!(err));
              (*pk, price_feed_result)
            })
            .collect()
        }
        Err(e) => {
          all_bank_pubkeys
            .iter()
            .map(|pk| (*pk, Err(anyhow::anyhow!("Failed to load oracle configs: {}", e))))
            .collect()
        }
      };
  
    let user_accounts: Vec<anyhow::Result<Self>> = marginfi_accounts
      .into_iter()
      .zip(account_pubkeys.iter())
      .map(|(opt_account, pubkey)| {
        let account = opt_account
          .ok_or_else(|| anyhow::anyhow!("Failed to load marginfi account {}", pubkey))?;
  
        let mut banks: Vec<BankAccount> = Vec::new();
        
        for balance in account.lending_account.get_active_balances_iter() {
          let bank = banks_map
            .get(&balance.bank_pk)
            .ok_or_else(|| anyhow::anyhow!("Missing bank {} for account {}", balance.bank_pk, pubkey))?;
          
          let price_feed = match price_feeds_map.get(&balance.bank_pk) {
            Some(Ok(pf)) => pf.clone(),
            Some(Err(e)) => {
              return Err(anyhow::anyhow!(
                "Failed to load price feed for bank {} in account {}: {}", 
                balance.bank_pk, 
                pubkey,
                e
              ));
            }
            None => {
              return Err(anyhow::anyhow!(
                "Missing price feed for bank {} in account {}", 
                balance.bank_pk, 
                pubkey
              ));
            }
          };
          
          banks.push(BankAccount {
            bank: *bank,
            price_feed,
            balance: *balance,
          });
        }
  
        let reconciled_emode_config = reconcile_emode_configs(
          banks
            .iter()
            .filter(|b| !b.balance.is_empty(BalanceSide::Liabilities))
            .map(|b| b.bank.emode.emode_config),
        );
  
        Ok(Self {
          account,
          bank_accounts: banks,
          emode_config: reconciled_emode_config,
        })
      })
      .collect();
  
    Ok(user_accounts)
  }
  
  pub async fn from_pubkey(rpc_client: &RpcClient, account_pubkey: &Pubkey) -> anyhow::Result<Self> {
    let results = Self::from_pubkeys(rpc_client, &[*account_pubkey]).await?;
    results.into_iter().next()
      .ok_or(anyhow::anyhow!("Failed to load account"))?
  }

  pub fn account(&self) -> &MarginfiAccount {
    &self.account
  }

  pub fn bank_accounts(&self) -> &[BankAccount] {
    &self.bank_accounts
  }

  /// returns lended value in usd
  pub fn asset_value(&self) -> anyhow::Result<I80F48> {
    let total_asset_value: I80F48 = self.bank_accounts.iter()
      .try_fold(I80F48::ZERO, |acc, bank_account| {
        let asset_value = bank_account.asset_value()?;
    
        anyhow::Ok(acc + asset_value)
      })?;

    anyhow::Ok(total_asset_value)
  }

  /// returns borrowed value in usd
  pub fn liability_value(&self) -> anyhow::Result<I80F48> {
    let total_liability_value: I80F48 = self.bank_accounts.iter()
      .try_fold(I80F48::ZERO, |acc, bank_account| {
        let liability_value = bank_account.liability_value()?;

        anyhow::Ok(acc + liability_value)
      })?;

    anyhow::Ok(total_liability_value)
  }

  pub fn maintenance(&self) -> anyhow::Result<I80F48> {
    let mut total_asset_value: I80F48 = I80F48::ZERO;
    let mut total_liability_value: I80F48 = I80F48::ZERO;
    for bank_account in &self.bank_accounts {
      let asset_value = bank_account.asset_value()?;
      let liability_value = bank_account.liability_value()?;

      // If an emode entry exists for this bank's emode tag in the reconciled config of
      // all borrowing banks, use its weight, otherwise use the weight designated on the
      // collateral bank itself. If the bank's weight is higher, always use that weight.
      let bank_asset_weight: I80F48 = bank_account.bank.config.asset_weight_maint.into();
      let asset_weight: I80F48 = if let Some(emode_entry) = self.emode_config.find_with_tag(bank_account.bank.emode.emode_tag) {
        let emode_weight = I80F48::from(emode_entry.asset_weight_maint);
        std::cmp::max(bank_asset_weight, emode_weight)
      } else {
        bank_asset_weight
      };
      let liability_weight: I80F48 = bank_account.bank.config.liability_weight_maint.into();

      total_asset_value += asset_value.checked_mul(asset_weight)
        .context("asset maintenance value calculation failed")?;
      total_liability_value += liability_value.checked_mul(liability_weight)
        .context("liability maintenance value calculation failed")?;
    }

    anyhow::Ok(total_asset_value - total_liability_value)
  }
}

#[derive(Clone)]
pub struct BankAccount {
  pub bank: Bank,
  pub price_feed: OraclePriceFeedAdapter,
  pub balance: Balance
}

impl BankAccount {
  pub fn asset_value(&self) -> anyhow::Result<I80F48> {
    if self.balance.is_empty(BalanceSide::Assets) {
      return anyhow::Ok(I80F48::ZERO);
    }
    let price = self.price_feed.get_price_of_type(
      OraclePriceType::RealTime,
      Some(super::types::PriceBias::Low),
      self.bank.config.oracle_max_confidence
    )?;

    let asset = self.bank.get_asset_amount(self.balance.asset_shares.into())
      .context("asset shares calculation failed")?;

    let asset_value_with_decimals = asset.checked_mul(price)
      .context("asset with decimals value calculation failed")?;

    let asset_value = self.bank.get_display_asset(asset_value_with_decimals)
      .context("asset value calculation failed")?;

    anyhow::Ok(asset_value)
  }

  pub fn liability_value(&self) -> anyhow::Result<I80F48> {
    if self.balance.is_empty(BalanceSide::Liabilities) {
      return anyhow::Ok(I80F48::ZERO);
    }
    let price = self.price_feed.get_price_of_type(
      OraclePriceType::RealTime,
      Some(super::types::PriceBias::Low),
      self.bank.config.oracle_max_confidence
    )?;

    let liability = self.bank.get_asset_amount(self.balance.liability_shares.into())
      .context("liability shares calculation failed")?;

    let liability_value_with_decimals = liability.checked_mul(price)
      .context("liability with decimals value calculation failed")?;

    let liability_value = self.bank.get_display_asset(liability_value_with_decimals)
      .context("liability value calculation failed")?;

    anyhow::Ok(liability_value)
  }
}