use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) url: String,
  pub(crate) ws_url: String,
}

impl Config {
  pub async fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let url = std::env::var("RPC_URL").context("\"RPC_URL\" is required")?;
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let config = Config {
      url,
      ws_url,
    };

    Ok(config)
  }
}