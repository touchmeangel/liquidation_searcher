use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) rpc_url: String,
}

impl Config {
  pub async fn open() -> anyhow::Result<Config> {
    dotenvy::dotenv().context("failed to load .env file")?;
    let rpc_url = std::env::var("RPC_URL").context("\"RPC_URL\" is required")?;
    let config = Config {
      rpc_url,
    };

    Ok(config)
  }
}