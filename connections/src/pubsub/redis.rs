use redis::{AsyncTypedCommands, aio::ConnectionManager, streams::StreamReadOptions};
use solana_pubkey::Pubkey;

const STREAM_KEY: &str = "account_stream";
const CONSUMER_GROUP: &str = "workers";

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
    let mut ids = Vec::new();
    
    let mut pipe = redis::pipe();
    for account in accounts {
      pipe.xadd(STREAM_KEY, "*", &[("pubkey", account.to_string())]);
    }
    
    let results: Vec<String> = pipe.query_async(&mut self.con).await?;
    ids.extend(results);
    
    Ok(ids)
  }
}

pub struct SubRedis {
  con: ConnectionManager
}

impl SubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let con = ConnectionManager::new(client).await?;

    Ok(Self { con })
  }

  pub async fn read(
    &mut self,
    consumer_name: &str,
    batch_size: usize,
    block_ms: usize,
  ) -> anyhow::Result<Vec<Pubkey>> {
    let opts = StreamReadOptions::default()
      .count(batch_size)
      .block(block_ms)
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
              items.push(pubkey);
            }
      }
    }
    
    Ok(items)
  }
}