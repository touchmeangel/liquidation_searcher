use anchor_lang::{InstructionData, prelude::*};

use crate::marginfi::consts::ix_discriminators;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PulseHealth;

impl Discriminator for PulseHealth {
  const DISCRIMINATOR: &'static [u8] = &ix_discriminators::LENDING_ACCOUNT_PULSE_HEALTH;
}

impl InstructionData for PulseHealth {
}

pub struct PulseHealthAccounts {
    pub marginfi_account: Pubkey,
}

impl ToAccountMetas for PulseHealthAccounts {
  fn to_account_metas(&self, is_signer: Option<bool>) -> Vec<AccountMeta> {
    vec![
      AccountMeta::new(self.marginfi_account, false),
    ]
  }
}