use solana_pubkey::Pubkey;
use solana_instruction::{AccountMeta, Instruction};

use crate::{consts::MARGINFI_PROGRAM_ID, marginfi::{LIQUIDITY_VAULT_AUTHORITY_SEED, ix_discriminators}};

pub fn make_repay_ix(
	group: Pubkey,
	marginfi_account: Pubkey,
	authority: Pubkey,
	bank: Pubkey,
	signer_token_account: Pubkey,
	liquidity_vault: Pubkey,
	token_program: Pubkey,
	amount: u64,
	repay_all: Option<bool>
) -> Instruction {
	let accounts = vec![
		AccountMeta::new_readonly(group, false),
		AccountMeta::new(marginfi_account, false),
		AccountMeta::new_readonly(authority, true),
		AccountMeta::new(bank, false),
		AccountMeta::new(signer_token_account, false),
		AccountMeta::new(liquidity_vault, false),
		AccountMeta::new_readonly(token_program, false),
	];

	let mut data = ix_discriminators::LENDING_ACCOUNT_REPAY.to_vec();
	data.extend_from_slice(&amount.to_le_bytes());
	
	match repay_all {
		Some(true) => {
			data.push(1); // Some
			data.push(1); // true
		}
		Some(false) => {
			data.push(1); // Some
			data.push(0); // false
		}
		None => {
			data.push(0); // None
		}
	}

	let repay_ix = Instruction {
		program_id: MARGINFI_PROGRAM_ID,
		accounts,
		data: data,
	};

	repay_ix
}