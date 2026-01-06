use bytemuck::Pod;
use anchor_lang::prelude::Pubkey;

pub fn parse_account<T: Pod>(
  data: &[u8],
  account_pubkey: &Pubkey,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
  let marginfi_account = bytemuck::try_from_bytes::<T>(&data[8..])
      .map_err(|e| format!("failed to deserialize: {:?}", e))?;
  
  Ok(*marginfi_account)
}