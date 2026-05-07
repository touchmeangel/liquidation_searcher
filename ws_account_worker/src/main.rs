mod config;

use std::{collections::HashMap, str::FromStr};
use std::sync::Arc;
use std::time::Duration;
use connections::Redis;
use futures_util::StreamExt;
use protocols::{consts::MARGINFI_PROGRAM_ID, marginfi::{MARGINFI_ACCOUNT_SEED, MarginfiAccount, discriminators}, utils::parse_account};
use redis::aio::ConnectionManager;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType};
use solana_client::rpc_response::{Response, RpcKeyedAccount};
use solana_pubkey::Pubkey;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use redis::AsyncTypedCommands;
use solana_commitment_config::CommitmentConfig;
use tokio::time::{self, interval};

use crate::config::Config;

#[tokio::main]
async fn main() {
  let config = Config::open().unwrap();

  let result = start(config).await;

  if let Err(err) = result {
    eprintln!("error: {err}");
    
    err.chain()
      .skip(1)
      .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}

async fn start(config: Config) -> anyhow::Result<()> {
  let mut redis = Redis::new(&config.redis_url).await?;
  let client = PubsubClient::new(&config.ws_url).await?;
  let config = RpcProgramAccountsConfig {
    filters: Some(vec![
      RpcFilterType::Memcmp(Memcmp::new(0, MemcmpEncodedBytes::Bytes(discriminators::ACCOUNT.to_vec())))
    ]),
    account_config: RpcAccountInfoConfig {
      encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
      commitment: Some(CommitmentConfig::processed()),
      data_slice: None,
      min_context_slot: None,
    },
    with_context: None,
    sort_results: None,
  };
  println!("stream established, listening");

  let (mut stream, _unsub) = client
    .program_subscribe(&MARGINFI_PROGRAM_ID, Some(config))
    .await?;

  while let Some(response) = stream.next().await {
    let pk = response.value.pubkey.clone();

    if let Err(err) = handle(&mut redis, response).await {
      println!("error handling {}: {err}", pk);
    }
  }

  Ok(())
}

async fn handle(redis: &mut Redis, response: Response<RpcKeyedAccount>) -> anyhow::Result<()> {
  let pk = Pubkey::from_str(&response.value.pubkey)?;
  let data = match response.value.account.data.decode() {
    Some(data) => data,
    None => anyhow::bail!("update with no data"),
  };

  let account = parse_account::<MarginfiAccount>(&data).map_err(|err| anyhow::anyhow!(err))?;
  
  if !redis.exists_multiple([&pk]).await?[0] {
    return Ok(());
  }

  let bank_accounts: Vec<_> = account
    .lending_account
    .balances
    .iter()
    .map(|balance| &balance.bank_pk)
    .collect();

  redis.rem_multiple([&pk]).await?;
  redis.add_multiple([&pk], vec![bank_accounts]).await?;

  Ok(())
}