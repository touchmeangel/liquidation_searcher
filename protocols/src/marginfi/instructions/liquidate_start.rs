use solana_pubkey::Pubkey;
use solana_instruction::{AccountMeta, Instruction};

use crate::{consts::MARGINFI_PROGRAM_ID, marginfi::{ix_discriminators}};

fn make_liquidation_start_ix(marginfi_account: Pubkey, liquidation_record: Pubkey, liquidation_receiver: Pubkey, instruction_sysvar: Pubkey) -> Instruction {
	let accounts = vec![
		AccountMeta::new(marginfi_account, false),
		AccountMeta::new(liquidation_record, false),
		AccountMeta::new(liquidation_receiver, false),
		AccountMeta::new_readonly(instruction_sysvar, false),
	];

	let start_liquidation_ix = Instruction {
		program_id: MARGINFI_PROGRAM_ID,
		accounts,
		data: ix_discriminators::START_LIQUIDATION.to_vec(),
	};

	start_liquidation_ix
}