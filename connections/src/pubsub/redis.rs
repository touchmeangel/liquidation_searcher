use std::time::Duration;

use redis::{AsyncTypedCommands, aio::{ConnectionManager, ConnectionManagerConfig}, streams::StreamReadOptions};
use solana_pubkey::Pubkey;

const QUEUE_KEY: &str = "accounts_queue";
const PENDING_SET: &str = "pending_accounts";

pub struct PubRedis {
  con: ConnectionManager
}

impl PubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let con = ConnectionManager::new(client).await?;

    let publish = Self { con };

    Ok(publish)
  }

  pub async fn publish(&mut self, accounts: &[Pubkey]) -> anyhow::Result<Vec<String>> {
    if accounts.is_empty() {
      return Ok(Vec::new());
    }
    
    let mut con = self.con.clone();
    let account_strings: Vec<String> = accounts.iter()
      .map(|a| a.to_string())
      .collect();
    
    let script = redis::Script::new(r"
      -- KEYS[1] = pending_set
      -- KEYS[2] = work_queue

      local pushed = {}

      for i, account in ipairs(ARGV) do
        if redis.call('SADD', KEYS[1], account) == 1 then
          redis.call('RPUSH', KEYS[2], account)
          table.insert(pushed, account)
        end
      end

      return pushed
    ");
    
    let results: Vec<String> = script
      .key(PENDING_SET)
      .key(QUEUE_KEY)
      .arg(&account_strings)
      .invoke_async(&mut con)
      .await?;
    
    Ok(results)
  }
}

pub struct SubRedis {
  con: ConnectionManager
}

impl SubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let config = ConnectionManagerConfig::new()
      .set_response_timeout(Some(Duration::from_secs(60)));
    let con = client.get_connection_manager_with_config(config).await?;

    let subscribe = Self { con };

    Ok(subscribe)
  }

  pub async fn read(
    &mut self,
    batch_size: usize,
  ) -> anyhow::Result<Vec<Pubkey>> {
    if batch_size == 0 {
      return Ok(Vec::new());
    }

    let script = redis::Script::new(r#"
      local n = tonumber(ARGV[1])
      local items = {}

      for i = 1, n do
        local v = redis.call('LPOP', KEYS[1])
        if not v then break end
        table.insert(items, v)
      end

      return items
    "#);

    let raw: Vec<String> = script
      .key(QUEUE_KEY)
      .arg(batch_size)
      .invoke_async(&mut self.con)
      .await?;

    let mut items = Vec::with_capacity(raw.len());

    for s in raw {
      if let Ok(pubkey) = s.parse::<Pubkey>() {
        items.push(pubkey);
      }
    }

    Ok(items)
  }
}