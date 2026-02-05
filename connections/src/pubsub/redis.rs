use redis::{AsyncTypedCommands, aio::ConnectionManager, streams::StreamReadOptions};
use solana_pubkey::Pubkey;

const STREAM_KEY: &str = "account_stream";
const PENDING_SET: &str = "pending_accounts";
const CONSUMER_GROUP: &str = "workers";

pub struct StreamMessage {
  stream_id: String,
  account: Pubkey
}

pub struct PubRedis {
  con: ConnectionManager
}

impl PubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let con = ConnectionManager::new(client).await?;

    let mut publish = Self { con };

    let _ = publish.create_consumer_group().await;
        
    Ok(publish)
  }

  async fn create_consumer_group(&mut self) -> anyhow::Result<()> {
    self.con.xgroup_create_mkstream(
      STREAM_KEY,
      CONSUMER_GROUP,
      "0"
    ).await?;
    Ok(())
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
      local pending_set = KEYS[1]
      local stream_key = KEYS[2]
      local ids = {}
      
      for i, account in ipairs(ARGV) do
        if redis.call('SADD', pending_set, account) == 1 then
          local id = redis.call('XADD', stream_key, '*', 'pubkey', account)
          table.insert(ids, id)
        end
      end
      
      return ids
    ");
    
    let results: Vec<String> = script
      .key(PENDING_SET)
      .key(STREAM_KEY)
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
    let con = ConnectionManager::new(client).await?;

    let mut subscribe = Self { con };

    let _ = subscribe.create_consumer_group().await;
        
    Ok(subscribe)
  }

  async fn create_consumer_group(&mut self) -> anyhow::Result<()> {
    self.con.xgroup_create_mkstream(
      STREAM_KEY,
      CONSUMER_GROUP,
      "0"
    ).await?;
    Ok(())
  }

  pub async fn read(
    &mut self,
    consumer_name: &str,
    batch_size: usize,
  ) -> anyhow::Result<Vec<StreamMessage>> {
    let opts = StreamReadOptions::default()
      .count(batch_size)
      .block(5000)
      .group(CONSUMER_GROUP, consumer_name);
    
    let results = match self.con.xread_options(
      &[STREAM_KEY],
      &[">"],
      &opts
    ).await? {
      Some(results) => results,
      None => return Ok(Vec::new()),
    };

    let mut items = Vec::new();
    
    for stream_key in results.keys {
      for stream_id in stream_key.ids {
        if let Some(redis::Value::BulkString(data)) = stream_id.map.get("pubkey")
          && let Ok(s) = String::from_utf8(data.clone())
            && let Ok(pubkey) = s.parse::<Pubkey>() {
              items.push(StreamMessage { stream_id: stream_id.id, account: pubkey });
            }
      }
    }
    
    Ok(items)
  }

  pub async fn ack(&mut self, items: &[StreamMessage]) -> anyhow::Result<usize> {
    if items.is_empty() {
      return Ok(0);
    }
    
    let mut con = self.con.clone();
    
    let message_ids: Vec<&str> = items.iter().map(|s| s.stream_id.as_str()).collect();
    let pubkey_strs: Vec<String> = items.iter().map(|s| s.account.to_string()).collect();
    
    let mut pipe = redis::pipe();
    pipe.xack(STREAM_KEY, CONSUMER_GROUP, &message_ids);
    pipe.srem(PENDING_SET, &pubkey_strs);
    
    let (acked, _removed): (usize, usize) = pipe.query_async(&mut con).await?;
    Ok(acked)
  }
}