mod config;

use std::{collections::HashMap, sync::atomic::AtomicU64};
use std::sync::Arc;
use anyhow::bail;
use connections::{PubRedis, Redis, SubRedis, queue_keys};
use futures_util::StreamExt;
use protocols::marginfi::{Bank, load_price_update_v2_checked_data};
use protocols::utils::parse_account;
use redis::aio::ConnectionManager;
use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use tokio::signal;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use redis::AsyncTypedCommands;
use solana_commitment_config::CommitmentConfig;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use uuid::Uuid;

use crate::config::Config;

type Subscriptions = Arc<Mutex<HashMap<String, JoinHandle<()>>>>;

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
  let mut worker = Worker::new(config).await?;

  worker.run().await
}

struct Worker {
  id: String,
  config: Config,
  pub_redis: PubRedis,
  sub_redis: SubRedis,
  redis: Redis,
  pubsub: Arc<PubsubClient>,
  rpc_client: RpcClient,
  heartbeat_redis: ConnectionManager,
  subs: Subscriptions,
}

impl Worker {
  pub async fn new(config: Config) -> anyhow::Result<Worker> {
    let id = Uuid::new_v4().to_string();

    let rpc_client = RpcClient::new(config.http_url.clone());
    let pubsub = Arc::new(PubsubClient::new(&config.ws_url).await?);
    let pub_redis = PubRedis::new(&config.redis_url).await?;
    let sub_redis = SubRedis::new(&config.redis_url).await?;
    let redis = Redis::new(&config.redis_url).await?;
    let client = redis::Client::open(config.heartbeat_url.clone())?;
    let con = ConnectionManager::new(client).await?;
    Ok(Worker { id, config, pub_redis, sub_redis, redis, heartbeat_redis: con, rpc_client, pubsub, subs: Arc::new(Mutex::new(HashMap::new())) })
  }

  pub async fn run(&mut self) -> anyhow::Result<()> {
    self.claim_existing().await?;

    let redis = self.heartbeat_redis.clone();
    let subs = self.subs.clone();
    let worker_id = self.id.clone();
    tokio::spawn(async move {
      heartbeat_loop(redis, worker_id, subs).await;
    });

    self.listen_for_events().await
  }

  async fn claim_existing(&mut self) -> anyhow::Result<()> {
    let accounts = self.redis.get_all_banks().await?;

    self.claim(accounts).await
  }

  async fn claim(&mut self, accounts: Vec<Pubkey>) -> anyhow::Result<()> {
    let mut i = 0;
    for account_batch in accounts.chunks(self.config.accounts_batch_size) {
      let accounts = match self.rpc_client.get_multiple_accounts(account_batch).await {
        Ok(accounts) => accounts,
        Err(err) => {
          println!("get_multiple_accounts resulted in error: {err}");
          continue
        },
      };
      
      for (result, pubkey) in accounts.into_iter().zip(account_batch) {
        let account = match result {
          Some(account) => account,
          None => continue,
        };

        let bank = match parse_account::<Bank>(&account.data) {
          Ok(bank) => bank,
          Err(err) => {
            println!("failed to parse account data of {}: {}", pubkey, err);
            continue
          },
        };
        self.try_claim(*pubkey, bank).await?;
        i += 1;
      }
    }
    
    println!("started listening to {} banks", i);
    Ok(())
  }

  async fn try_claim(&mut self, pubkey: Pubkey, bank: Bank) -> anyhow::Result<bool> {
    let lease_key = format!("bank:lease:{}", pubkey);

    let claimed: bool = redis::cmd("SET")
      .arg(&lease_key)
      .arg(&self.id)
      .arg("NX")
      .arg("EX")
      .arg(30u64)
      .query_async(&mut self.heartbeat_redis)
      .await?;

    if claimed {
      self.start_subscription(pubkey, bank).await;
    }

    Ok(claimed)
  }

  async fn start_subscription(&self, pubkey: Pubkey, bank: Bank) {
    let pubsub_clone = Arc::clone(&self.pubsub);
    let pub_redis_clone = self.pub_redis.clone();
    let redis_clone = self.redis.clone();
    let subs = self.subs.clone();
    let pubkey_clone = pubkey;

    let handle = tokio::spawn(async move {
      if let Err(e) = subscribe_to_bank(pubsub_clone, pub_redis_clone, redis_clone, pubkey_clone, bank).await {
        eprintln!("[{}] subscription error: {}", pubkey_clone, e);
      }
    });

    subs.lock().await.insert(pubkey.to_string(), handle);
  }

  async fn drop_subscription(&self, pubkey: Pubkey) {
    let mut map = self.subs.lock().await;
    if let Some(handle) = map.remove(&pubkey.to_string()) {
      handle.abort();
    }
  }

  async fn listen_for_events(&mut self) -> anyhow::Result<()> {
    let mut rem_sub_redis = self.sub_redis.clone();
    loop {
      tokio::select! {
        result = self.sub_redis.builder::<()>(queue_keys::BANK_ADD_QUEUE, self.config.accounts_batch_size).recv() => {
          let results = match result {
            Ok(messages) => messages,
            Err(err) => {
              println!("error while reading: {}", err);
              continue
            },
          };
  
          for result in &results {
            if let Err(err) = result {
              println!("failed to accept a message: {}", err);
            }
          }
  
          let bank_accounts: Vec<_> = results.into_iter().filter_map(|result| result.map(|(pk, _)| pk).ok()).collect();
          
          if bank_accounts.is_empty() {
            continue;
          }
          
          let banks_amount = bank_accounts.len();
          if let Err(err) = self.claim(bank_accounts).await {
            eprintln!("failed to claim {} banks: {}", banks_amount, err);
          }
        }
        result = rem_sub_redis.builder::<()>(queue_keys::BANK_REM_QUEUE, self.config.accounts_batch_size).recv() => {
          let results = match result {
            Ok(messages) => messages,
            Err(err) => {
              println!("error while reading: {}", err);
              continue
            },
          };
  
          for result in &results {
            if let Err(err) = result {
              println!("failed to accept a message: {}", err);
            }
          }
  
          let bank_accounts: Vec<_> = results.into_iter().filter_map(|result| result.map(|(pk, _)| pk).ok()).collect();
          
          if bank_accounts.is_empty() {
            continue;
          }
          
          let banks_amount = bank_accounts.len();
          for bank in bank_accounts {
            self.drop_subscription(bank).await;
          }
          println!("stopped listening to {} banks", banks_amount);
        }
        _ = signal::ctrl_c() => {
          println!("shutting down");
          break;
        }
      }
    }

    Ok(())
  }
}

async fn heartbeat_loop(
  mut redis: redis::aio::ConnectionManager,
  worker_id: String,
  subs: Subscriptions,
) {
  let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
  loop {
    interval.tick().await;
    let owned: Vec<String> = subs.lock().await.keys().cloned().collect();

    for pubkey in owned {
      let lease_key = format!("bank:lease:{}", pubkey);

      let owner: Option<String> = redis.get(&lease_key).await.unwrap_or(None);
      if matches!(owner.as_deref(), Some(id) if id != worker_id) {
        println!("{} is now owned by another process, aborting...", pubkey);
        let join_handle = subs.lock().await.remove(&pubkey);
        if let Some(j) = join_handle {
          j.abort();
        }
        continue;
      }
      
      let _ = redis.expire(&lease_key, 30).await.unwrap_or(false);
    }
  }
}

async fn subscribe_to_bank(client: Arc<PubsubClient>, mut pub_redis: PubRedis, mut redis: Redis, pubkey: Pubkey, bank: Bank) -> anyhow::Result<()> {
  let oracle_setup = bank.config.oracle_setup.validate().map_err(|err| anyhow::anyhow!(err))?;
  // KaminoPythPush
  // KaminoSwitchboardPull
  // SwitchboardPull
  match oracle_setup {
    protocols::marginfi::OracleSetup::SwitchboardV2 => return Ok(()),
    protocols::marginfi::OracleSetup::PythPushOracle => {
      let feed = match bank.config.oracle_keys.first() {
        Some(feed) => feed,
        None => bail!("invalid bank oracle keys {}", pubkey),
      };

      let config = RpcAccountInfoConfig {
        encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
        commitment: Some(CommitmentConfig::processed()),
        data_slice: None,
        min_context_slot: None,
      };

      let (mut stream, _unsub) = client
        .account_subscribe(feed, Some(config))
        .await.expect("failed to sub");
      
      let last_posted_slot = Arc::new(AtomicU64::new(0));
      while let Some(response) = stream.next().await {
        let account_data = match response.value.data {
          solana_account_decoder::UiAccountData::Binary(data, _) => {
            base64::decode(data)?
          }
          _ => continue,
        };

        let price_update = match load_price_update_v2_checked_data(&account_data) {
          Ok(p) => p,
          Err(e) => {
            eprintln!("failed to deserialize PriceUpdateV2: {}", e);
            continue;
          }
        };

        let posted_slot = price_update.price_message.publish_time as u64;
        let prev = last_posted_slot.load(std::sync::atomic::Ordering::Relaxed);

        if posted_slot <= prev {
          continue;
        }

        last_posted_slot.store(posted_slot, std::sync::atomic::Ordering::Relaxed);

        if let Err(err) = trigger(&mut pub_redis, &mut redis, &pubkey).await {
          eprintln!("failed to trigger {} bank: {}", pubkey, err);
          continue;
        }
      }
    },
    protocols::marginfi::OracleSetup::SwitchboardPull => return Ok(()),
    protocols::marginfi::OracleSetup::StakedWithPythPush => return Ok(()),
    protocols::marginfi::OracleSetup::KaminoPythPush => return Ok(()),
    protocols::marginfi::OracleSetup::KaminoSwitchboardPull => return Ok(()),
    protocols::marginfi::OracleSetup::DriftPythPull => return Ok(()),
    protocols::marginfi::OracleSetup::DriftSwitchboardPull => return Ok(()),
    protocols::marginfi::OracleSetup::SolendPythPull => return Ok(()),
    protocols::marginfi::OracleSetup::SolendSwitchboardPull => return Ok(()),
    protocols::marginfi::OracleSetup::PythLegacy => return Ok(()),
    protocols::marginfi::OracleSetup::Fixed => return Ok(()),
    protocols::marginfi::OracleSetup::None => return Ok(()),
  };

  Ok(())
}

async fn trigger(pub_redis: &mut PubRedis, redis: &mut Redis, bank: &Pubkey) -> anyhow::Result<()> {
  let accounts = redis.get_accounts_by_bank(bank).await?;
  println!("* triggering {} accounts", accounts.len());
  let _ = pub_redis.builder::<()>(queue_keys::CHECK_QUEUE).items(accounts.into_iter().map(|account| (account, ()))).send().await?;

  anyhow::Ok(())
}