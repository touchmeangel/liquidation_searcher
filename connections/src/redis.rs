use redis::{AsyncTypedCommands, aio::ConnectionManager};
use solana_pubkey::Pubkey;

const KEY: &str = "accounts";

pub struct Redis {
  con: ConnectionManager
}

impl Redis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let con = ConnectionManager::new(client).await?;

    Ok(Self { con })
  }

  pub async fn add<'a, I>(&mut self, accounts: I) -> anyhow::Result<usize>
    where I: IntoIterator<Item = &'a Pubkey> {
    let strings: Vec<String> = accounts.into_iter().map(ToString::to_string).collect();
    let result = self.con.sadd(KEY, &strings).await?;
    Ok(result)
  }

  pub async fn rem<'a, I>(&mut self, accounts: I) -> anyhow::Result<usize>
    where I: IntoIterator<Item = &'a Pubkey> {
    let strings: Vec<String> = accounts.into_iter().map(ToString::to_string).collect();
    let result = self.con.srem(KEY, &strings).await?;
    Ok(result)
  }

  pub async fn get_all(&mut self) -> anyhow::Result<Vec<Pubkey>> {
    let strings = self.con.smembers(KEY).await?;
    
    let pubkeys: Result<Vec<Pubkey>, _> = strings
      .iter()
      .map(|s| s.parse::<Pubkey>())
      .collect();
    
    Ok(pubkeys?)
  }
}