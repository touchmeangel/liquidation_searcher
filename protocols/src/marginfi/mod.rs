mod instructions;
mod user;
mod types;
mod consts;
mod errors;
mod events;
mod filter;
mod macros;
mod prelude;
mod wrapped_i80f48;

use anchor_lang::Discriminator;
pub use errors::*;
use events::*;
use solana_account_decoder::{UiAccountEncoding, UiDataSliceConfig};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{MemcmpEncodedBytes, Memcmp, RpcFilterType};
use wrapped_i80f48::*;
pub use consts::*;
pub use types::*;
pub use filter::*;
pub use user::*;

use std::sync::Arc;

use solana_pubkey::Pubkey;
use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use anchor_client::{Client, Cluster, Program};
use anchor_client::solana_sdk::signature::Keypair;

use crate::consts::MARGINFI_PROGRAM_ID;
use crate::utils::parse_account;

pub struct Marginfi {
  pubsub: PubsubClient,
  rpc_client: RpcClient,
  client: Client<Arc<Keypair>>,
  program: Program<Arc<Keypair>>,
}

impl Marginfi {
  pub async fn new(http_url: String, ws_url: String) -> anyhow::Result<Self> {
    let pubsub = PubsubClient::new(&ws_url).await?;
    let payer = Arc::new(Keypair::new());
    let client = Client::new(Cluster::Custom(http_url, ws_url), payer);
    let program = client.program(MARGINFI_PROGRAM_ID)?;
    let rpc_client = program.rpc();

    anyhow::Ok(Self { pubsub, rpc_client, client, program })
  }

	pub fn rpc_ref(&self) -> &RpcClient {
		&self.rpc_client
	}

  pub async fn get_all_accounts(&self) -> anyhow::Result<Vec<Pubkey>> {
    let filters = vec![
      RpcFilterType::Memcmp(Memcmp::new(
        0,
        MemcmpEncodedBytes::Bytes(Vec::from(MarginfiAccount::DISCRIMINATOR))
      )),
    ];

    let config = RpcProgramAccountsConfig {
      filters: Some(filters),
      account_config: RpcAccountInfoConfig {
        encoding: Some(UiAccountEncoding::Base64),
        data_slice: Some(UiDataSliceConfig {
          offset: 0,
          length: 0,
        }),
        commitment: Some(CommitmentConfig::confirmed()),
        min_context_slot: None,
      },
      with_context: None,
      sort_results: None,
    };

    let accounts = self.rpc_client
      .get_program_accounts_with_config(&MARGINFI_PROGRAM_ID, config)
      .await?;

    anyhow::Ok(accounts.into_iter().map(|(pk, _)| pk).collect())
  }

  pub async fn load_users(&self, pubkeys: &[Pubkey]) -> anyhow::Result<Vec<anyhow::Result<MarginfiUser>>> {
    MarginfiUser::from_pubkeys(&self.rpc_client, pubkeys).await
  }

  pub async fn get_fee_state(&self) -> anyhow::Result<FeeState> {
    let (expected_key, expected_bump) = Pubkey::find_program_address(&[FEE_STATE_SEED.as_bytes()], &MARGINFI_PROGRAM_ID);
    let account = self.rpc_client.get_account(&expected_key).await?;
    let fee_state = parse_account::<FeeState>(&account.data).map_err(|err| anyhow::anyhow!(err))?;

    anyhow::Ok(fee_state)
  }
}

fn parse_anchor_event<T: anchor_lang::AnchorDeserialize>(data: &str) -> anyhow::Result<T> {
  use base64::{Engine as _, engine::general_purpose};
  let decoded = general_purpose::STANDARD.decode(data)?;
  let event_data = &decoded[8..];
  Ok(T::deserialize(&mut &event_data[..])?)
}