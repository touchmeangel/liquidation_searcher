use solana_pubkey::Pubkey;
use solana_instruction::{AccountMeta, Instruction};
use solana_sdk_ids::system_program;

use crate::{consts::MARGINFI_PROGRAM_ID, marginfi::{FEE_STATE_SEED, ix_discriminators}};

pub fn make_end_liquidation_ix(marginfi_account: Pubkey, liquidation_record: Pubkey, liquidation_receiver: Pubkey, global_fee_wallet: Pubkey) -> Instruction {
	let (fee_state, expected_bump) = Pubkey::find_program_address(&[FEE_STATE_SEED.as_bytes()], &MARGINFI_PROGRAM_ID);
	let accounts = vec![
		AccountMeta::new(marginfi_account, false),
		AccountMeta::new(liquidation_record, false),
		AccountMeta::new(liquidation_receiver, true),
		AccountMeta::new(fee_state, false),
		AccountMeta::new(global_fee_wallet, false),
		AccountMeta::new_readonly(system_program::ID, false),
	];

	let end_liquidation_ix = Instruction {
		program_id: MARGINFI_PROGRAM_ID,
		accounts,
		data: ix_discriminators::END_LIQUIDATION.to_vec(),
	};

	end_liquidation_ix
}