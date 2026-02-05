use anyhow::Context;
use futures_core::Stream;
use futures_util::StreamExt;
use redis::{AsyncTypedCommands, aio::{ConnectionManager, PubSub}};
use solana_pubkey::Pubkey;

const KEY: &str = "check_queue";

pub struct PubRedis {
  con: ConnectionManager
}

impl PubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let con = ConnectionManager::new(client).await?;

    Ok(Self { con })
  }

  pub async fn publish(&mut self, account: Pubkey) -> anyhow::Result<usize> {
    let result = self.con.publish(KEY, account.to_string()).await?;
    Ok(result)
  }
}

pub struct SubRedis {
  pubsub: PubSub
}

impl SubRedis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let pubsub = client.get_async_pubsub().await?;

    Ok(Self { pubsub })
  }

  pub async fn subscribe(&mut self) -> anyhow::Result<impl Stream<Item = anyhow::Result<Pubkey>> + '_> {
    self.pubsub.subscribe(KEY).await?;
    let stream = self.pubsub.on_message()
      .map(|msg| -> anyhow::Result<Pubkey> {
        let payload: String = msg.get_payload()?;
        payload.parse::<Pubkey>().context("invalid account")
      });
    
    Ok(stream)
  }
}