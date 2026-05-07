use anyhow::Context;
use solana_sdk::signature::Keypair;

#[derive(Debug)]
pub struct Config {
  pub(crate) http_url: String,
  pub(crate) ws_url: String,
  pub(crate) pubsub_url: String,
	pub(crate) jup_swap_api_v2: String,
	pub(crate) payer: Keypair,
  pub(crate) capacity: usize,
  pub(crate) asset_haircut: f64,
  pub(crate) asset_swap_haircut: f64,
}

impl Config {
  pub fn open() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let http_url = std::env::var("HTTP_URL").context("\"HTTP_URL\" is required")?;
    let ws_url = std::env::var("WS_URL").context("\"WS_URL\" is required")?;
    let pubsub_url = std::env::var("PUBSUB_CONNECTION").context("\"PUBSUB_CONNECTION\" is required")?;
    let jup_swap_api_v2 = std::env::var("JUP_SWAP_API_V2").context("\"JUP_SWAP_API_V2\" is required")?;
    let payer_string = std::env::var("PAYER").context("\"PAYER\" is required")?;
		let payer = Keypair::from_base58_string(&payer_string);
    let capacity = env_usize("CAPACITY", 1).context("invalid \"CAPACITY\" value")?;
    let asset_haircut = std::env::var("ASSET_HAIRCUT").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.95);
    let asset_swap_haircut = std::env::var("ASSET_SWAP_HAIRCUT").ok().filter(|s| !s.is_empty()).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.95);
    let config = Config {
      http_url,
      ws_url,
      pubsub_url,
			jup_swap_api_v2,
			payer,
      capacity,
      asset_haircut,
      asset_swap_haircut
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