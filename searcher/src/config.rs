use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) http_url: String,
  pub(crate) ws_url: String,
  pub(crate) redis_url: String
}

impl Config {
  pub async fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let http_url = std::env::var("HTTP_URL").context("\"HTTP_URL\" is required")?;
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let redis_url = std::env::var("REDIS_CONNECTION").context("\"REDIS_CONNECTION\" is required")?;
    let config = Config {
      http_url,
      ws_url,
      redis_url,
    };

    Ok(config)
  }
}