use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Config {
  pub(crate) http_url: String,
  pub(crate) ws_url: String,
  pub(crate) redis_url: String,
  pub(crate) pubsub_url: String,
  pub(crate) capacity: usize,
  pub(crate) accounts_batch_size: usize,
  pub(crate) min_asset_value: Option<f64>,
  pub(crate) max_asset_value: Option<f64>,
  pub(crate) min_liability_value: Option<f64>,
  pub(crate) max_liability_value: Option<f64>,
  pub(crate) min_maint_percentage: Option<f64>,
  pub(crate) max_maint_percentage: Option<f64>,
  pub(crate) min_maint: Option<f64>,
  pub(crate) max_maint: Option<f64>
}

impl Config {
  pub fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let http_url = std::env::var("HTTP_URL").context("\"HTTP_URL\" is required")?;
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let redis_url = std::env::var("REDIS_CONNECTION").context("\"REDIS_CONNECTION\" is required")?;
    let pubsub_url = std::env::var("PUBSUB_CONNECTION").context("\"PUBSUB_CONNECTION\" is required")?;
    let capacity = env_usize("CAPACITY", 1).context("invalid \"CAPACITY\" value")?;
    let accounts_batch_size = env_usize("ACCOUNTS_BATCH_SIZE", 1000).context("invalid \"ACCOUNTS_BATCH_SIZE\" value")?;
    let min_asset_value = std::env::var("MIN_ASSET_VALUE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let max_asset_value = std::env::var("MAX_ASSET_VALUE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let min_liability_value = std::env::var("MIN_LIABILITY_VALUE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let max_liability_value = std::env::var("MAX_LIABILITY_VALUE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let min_maint_percentage = std::env::var("MIN_MAINT_PERCENTAGE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let max_maint_percentage = std::env::var("MAX_MAINT_PERCENTAGE").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let min_maint = std::env::var("MIN_MAINT").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let max_maint = std::env::var("MAX_MAINT").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok());
    let config = Config {
      http_url,
      ws_url,
      redis_url,
      pubsub_url,
      capacity,
      accounts_batch_size,
      min_asset_value,
      max_asset_value,
      min_liability_value,
      max_liability_value,
      min_maint_percentage,
      max_maint_percentage,
      min_maint,
      max_maint,
    };

    Ok(config)
  }
}

fn env_usize(name: &str, default: usize) -> Result<usize, std::num::ParseIntError> {
  std::env::var(name)
    .ok()
    .filter(|s| !s.is_empty())
    .map(|s| s.parse::<usize>())
    .transpose()
    .map(|opt| opt.unwrap_or(default))
}