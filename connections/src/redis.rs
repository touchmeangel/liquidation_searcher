use redis::{AsyncTypedCommands, aio::ConnectionManager};
use solana_pubkey::Pubkey;

const ACCOUNT_KEY: &str = "accounts";
const BANK_KEY: &str = "banks";

#[derive(Clone)]
pub struct Redis {
  con: ConnectionManager
}

impl Redis {
  pub async fn new(connection_info: &str) -> anyhow::Result<Self> {
    let client = redis::Client::open(connection_info)?;
    let con = ConnectionManager::new(client).await?;

    Ok(Self { con })
  }

  pub async fn exists_multiple<'a, I>(&mut self, accounts: I) -> anyhow::Result<Vec<bool>>
    where I: IntoIterator<Item = &'a Pubkey> {
    let keys: Vec<String> = accounts.into_iter().map(|a| a.to_string()).collect();

    let result: Vec<bool> = redis::cmd("SMISMEMBER")
      .arg(ACCOUNT_KEY)
      .arg(keys)
      .query_async(&mut self.con)
      .await?;

    Ok(result)
  }

  const ADD_BANK_SCRIPT: &str = r#"
    local added_banks = {}
    local accounts_added = 0

    local acc_count = tonumber(ARGV[1])
    local idx = 2

    for i = 1, acc_count do
      local account = ARGV[idx]
      idx = idx + 1

      if redis.call("SADD", KEYS[1], account) == 1 then
        accounts_added = accounts_added + 1
      end

      local bank_count = tonumber(ARGV[idx])
      idx = idx + 1

      for j = 1, bank_count do
        local bank = ARGV[idx]
        idx = idx + 1

        if redis.call("SADD", KEYS[2], bank) == 1 then
          table.insert(added_banks, bank)
        end

        redis.call("SADD", "account:banks:" .. account, bank)
        redis.call("SADD", "bank:accounts:" .. bank, account)
      end
    end

    local result = { tostring(accounts_added) }
    for _, bank in ipairs(added_banks) do
      table.insert(result, bank)
    end

    return result
  "#;

  pub async fn add_multiple<'a, T, I>(&mut self, accounts: T, bank_accounts: Vec<I>) -> anyhow::Result<(Vec<Pubkey>, usize)>
    where T: IntoIterator<Item = &'a Pubkey>, I: IntoIterator<Item = &'a Pubkey> {
    let accounts: Vec<&Pubkey> = accounts.into_iter().collect();
    let bank_accounts: Vec<Vec<&Pubkey>> = bank_accounts
      .into_iter()
      .map(|banks| banks.into_iter().collect())
      .collect();

    let args: Vec<String> = std::iter::once(accounts.len().to_string())
      .chain(accounts.iter().zip(bank_accounts.iter()).flat_map(|(account, banks)| {
        std::iter::once(account.to_string())
          .chain(std::iter::once(banks.len().to_string()))
          .chain(banks.iter().map(|b| b.to_string()))
      }))
      .collect();

    let mut result = redis::Script::new(Self::ADD_BANK_SCRIPT)
      .key(ACCOUNT_KEY)
      .key(BANK_KEY)
      .arg(args)
      .invoke_async::<Vec<String>>(&mut self.con)
      .await?
      .into_iter();

    let accounts_added: usize = result
      .next()
      .ok_or_else(|| anyhow::anyhow!("script returned empty response"))?
      .parse()?;

    let added_banks = result
      .map(|s| s.parse())
      .collect::<Result<Vec<Pubkey>, _>>()?;

    Ok((added_banks, accounts_added))
  }

  const REM_ACCOUNTS_SCRIPT: &str = r#"
    local accounts = KEYS
    local account_key = ARGV[1]
    local bank_key = ARGV[2]

    local removed = 0
    local removed_banks = {}

    for _, account in ipairs(accounts) do
      local banks = redis.call('SMEMBERS', 'account:banks:' .. account)

      for _, bank in ipairs(banks) do
        redis.call('SREM', 'bank:accounts:' .. bank, account)
      end

      redis.call('DEL', 'account:banks:' .. account)

      removed = removed + redis.call('SREM', account_key, account)

      if redis.call('SCARD', 'bank:accounts:' .. bank) == 0 then
        redis.call('SREM', bank_key, bank)
        redis.call('DEL', 'bank:accounts:' .. bank)
        table.insert(removed_banks, bank)
      end
    end

    local result = { tostring(removed) }
    for _, bank in ipairs(removed_banks) do
      table.insert(result, bank)
    end

    return result
  "#;

  pub async fn rem_multiple<'a, I>(&mut self, accounts: I) -> anyhow::Result<(Vec<Pubkey>, usize)>
    where I: IntoIterator<Item = &'a Pubkey> {
    let accounts: Vec<String> = accounts.into_iter().map(ToString::to_string).collect();

    let script = redis::Script::new(Self::REM_ACCOUNTS_SCRIPT);

    let mut invocation = script.prepare_invoke();
    for account in &accounts {
      invocation.key(account);
    }
    invocation.arg(ACCOUNT_KEY);
    invocation.arg(BANK_KEY);

    let mut result = invocation.invoke_async::<Vec<String>>(&mut self.con).await?.into_iter();

    let accounts_removed: usize = result
      .next()
      .ok_or_else(|| anyhow::anyhow!("script returned empty response"))?
      .parse()?;

    let removed_banks = result
      .map(|s| s.parse())
      .collect::<Result<Vec<Pubkey>, _>>()?;

    Ok((removed_banks, accounts_removed))
  }

  pub async fn get_all_accounts(&mut self) -> anyhow::Result<Vec<Pubkey>> {
    let strings = self.con.smembers(ACCOUNT_KEY).await?;
    
    let pubkeys: Result<Vec<Pubkey>, _> = strings
      .iter()
      .map(|s| s.parse::<Pubkey>())
      .collect();
    
    Ok(pubkeys?)
  }

  pub async fn get_accounts_by_bank(&mut self, bank: &Pubkey) -> anyhow::Result<Vec<Pubkey>> {
    let strings = self.con.smembers(format!("bank:accounts:{}", bank)).await?;

    let pubkeys: Result<Vec<Pubkey>, _> = strings
      .iter()
      .map(|s| s.parse::<Pubkey>())
      .collect();

    Ok(pubkeys?)
  }

  pub async fn get_all_banks(&mut self) -> anyhow::Result<Vec<Pubkey>> {
    let strings = self.con.smembers(BANK_KEY).await?;
    
    let pubkeys: Result<Vec<Pubkey>, _> = strings
      .iter()
      .map(|s| s.parse::<Pubkey>())
      .collect();
    
    Ok(pubkeys?)
  }
}