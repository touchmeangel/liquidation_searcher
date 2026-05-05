use solana_pubkey::Pubkey;
use solana_instruction::{AccountMeta, Instruction};

use crate::{consts::MARGINFI_PROGRAM_ID, marginfi::{ix_discriminators}};

pub fn make_start_liquidation_ix(marginfi_account: Pubkey, liquidation_record: Pubkey, liquidation_receiver: Pubkey) -> Instruction {
	let accounts = vec![
		AccountMeta::new(marginfi_account, false),
		AccountMeta::new(liquidation_record, false),
		AccountMeta::new(liquidation_receiver, false),
		AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
	];

	let start_liquidation_ix = Instruction {
		program_id: MARGINFI_PROGRAM_ID,
		accounts,
		data: ix_discriminators::START_LIQUIDATION.to_vec(),
	};

	start_liquidation_ix
}