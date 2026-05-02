use std::time::Duration;

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

pub mod queue_keys {
  pub const ADD_QUEUE: &str = "accounts_add_queue";
  pub const CHECK_QUEUE: &str = "accounts_check_queue";
  pub const LIQUIDATION_QUEUE: &str = "accounts_liquidation_queue";
  pub const REM_QUEUE: &str = "accounts_rem_queue";
  pub const BANK_ADD_QUEUE: &str = "bank_add_queue";
  pub const BANK_REM_QUEUE: &str = "bank_rem_queue";
}

fn queue_to_pending_set(queue: &str) -> String {
  format!("{}_pending", queue)
}

pub struct PublishBuilder<'a, T: Serialize> {
  redis: &'a mut PubRedis,
  queue: &'a str,
  items: Vec<(Pubkey, T)>,
}

impl<'a, T: Serialize> PublishBuilder<'a, T> {
  pub fn item(mut self, pubkey: Pubkey, payload: T) -> Self {
    self.items.push((pubkey, payload));
    self
  }

  pub fn items(mut self, items: impl IntoIterator<Item = (Pubkey, T)>) -> Self {
    self.items.extend(items);
    self
  }

  pub async fn send(self) -> anyhow::Result<Vec<Pubkey>> {
    self.redis.publish(self.queue, &self.items).await
  }
}

#[derive(Clone)]
pub struct PubRedis {
  con: ConnectionManager
}

impl PubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let config = ConnectionManagerConfig::new()
      .set_response_timeout(Some(Duration::from_secs(60)));
    let con = client.get_connection_manager_with_config(config).await?;

    let publish = Self { con };

    Ok(publish)
  }

  pub fn builder<'a, T: Serialize>(&'a mut self, queue: &'a str) -> PublishBuilder<'a, T> {
    PublishBuilder {
      redis: self,
      queue,
      items: Vec::new(),
    }
  }

  pub async fn publish<T: Serialize>(
    &mut self,
    queue: &str,
    items: &[(Pubkey, T)],
  ) -> anyhow::Result<Vec<Pubkey>> {
    if items.is_empty() {
      return Ok(Vec::new());
    }

    let mut args: Vec<String> = Vec::with_capacity(items.len() * 2);
    for (pubkey, payload) in items {
      let entry = bincode::serialize(&(pubkey.to_bytes(), payload))?;
      let pubkey_str = pubkey.to_string();
      let list_value = format!("{}|{}", pubkey_str, hex::encode(entry));
      args.push(pubkey_str);
      args.push(list_value);
    }

    let pending_set = queue_to_pending_set(queue);
    let script = redis::Script::new(r"
      local pushed = {}
      local n = #ARGV / 2

      for i = 1, n do
        local account = ARGV[(i-1)*2 + 1]
        local entry   = ARGV[(i-1)*2 + 2]
        if redis.call('SADD', KEYS[1], account) == 1 then
          redis.call('RPUSH', KEYS[2], entry)
          table.insert(pushed, account)
        end
      end

      return pushed
    ");

    let mut inv = script.prepare_invoke();
    inv.key(&pending_set).key(queue);
    for arg in &args {
      inv.arg(arg);
    }

    let raw: Vec<String> = inv
      .invoke_async(&mut self.con.clone())
      .await?;

    Ok(raw.iter().filter_map(|s| s.parse().ok()).collect())
  }
}

#[derive(Clone)]
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

  pub fn builder<'a, T: for<'de> Deserialize<'de>>(&'a mut self, queue: &'a str, batch_size: usize) -> ReadBuilder<'a, T> {
    ReadBuilder {
      redis: self,
      queue,
      batch_size,
      _marker: std::marker::PhantomData
    }
  }

  pub async fn read<T: for<'de> Deserialize<'de>>(
    &mut self,
    queue: &str,
    batch_size: usize,
  ) -> anyhow::Result<Vec<anyhow::Result<(Pubkey, T)>>> {
    if batch_size == 0 {
      return Ok(Vec::new());
    }

    let pending_set = queue_to_pending_set(queue);
    let script = redis::Script::new(r#"
      local n     = tonumber(ARGV[1])
      local items = {}

      for i = 1, n do
        local entry = redis.call('LPOP', KEYS[2])
        if not entry then break end

        local sep = string.find(entry, '|')
        if sep then
          local account = string.sub(entry, 1, sep - 1)
          redis.call('SREM', KEYS[1], account)
        end

        table.insert(items, entry)
      end

      return items
    "#);

    let mut inv = script.prepare_invoke();
    inv.key(&pending_set).key(queue).arg(batch_size);

    let raw: Vec<String> = inv
      .invoke_async(&mut self.con)
      .await?;

    let items = raw
      .into_iter()
      .map(|s| -> anyhow::Result<(Pubkey, T)> {
        let (pubkey_str, hex_payload) = s
          .split_once('|')
          .ok_or_else(|| anyhow::anyhow!("malformed entry, missing '|': {s}"))?;

        let pubkey: Pubkey = pubkey_str.parse()?;
        let bytes = hex::decode(hex_payload)?;
        let (_, payload): ([u8; 32], T) = bincode::deserialize(&bytes)?;

        Ok((pubkey, payload))
      })
      .collect();

    Ok(items)
  }
}

pub struct ReadBuilder<'a, T: for<'de> Deserialize<'de>> {
  redis: &'a mut SubRedis,
  queue: &'a str,
  batch_size: usize,
  _marker: std::marker::PhantomData<T>,
}

impl<'a, T: for<'de> Deserialize<'de>> ReadBuilder<'a, T> {
  pub fn batch_size(mut self, n: usize) -> Self {
      self.batch_size = n;
      self
  }

  pub async fn recv(self) -> anyhow::Result<Vec<anyhow::Result<(Pubkey, T)>>> {
      self.redis.read(self.queue, self.batch_size).await
  }
}