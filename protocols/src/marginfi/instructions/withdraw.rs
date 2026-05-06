use solana_pubkey::Pubkey;
use solana_instruction::{AccountMeta, Instruction};

use crate::{consts::MARGINFI_PROGRAM_ID, marginfi::{LIQUIDITY_VAULT_AUTHORITY_SEED, ix_discriminators}};

pub fn make_withdraw_ix(
	group: Pubkey,
	marginfi_account: Pubkey,
	authority: Pubkey,
	bank: Pubkey,
	destination_token_account: Pubkey,
	liquidity_vault: Pubkey,
	token_program: Pubkey,
	amount: u64,
	withdraw_all: Option<bool>
) -> Instruction {
	let (bank_liquidity_vault_authority, expected_bump) = Pubkey::find_program_address(&[LIQUIDITY_VAULT_AUTHORITY_SEED.as_bytes()], &MARGINFI_PROGRAM_ID);
	let accounts = vec![
		AccountMeta::new_readonly(group, false),
		AccountMeta::new(marginfi_account, false),
		AccountMeta::new_readonly(authority, true),
		AccountMeta::new(bank, false),
		AccountMeta::new(destination_token_account, false),
		AccountMeta::new_readonly(bank_liquidity_vault_authority, false),
		AccountMeta::new(liquidity_vault, false),
		AccountMeta::new_readonly(token_program, false),
	];

	let mut data = ix_discriminators::LENDING_ACCOUNT_WITHDRAW.to_vec();
	data.extend_from_slice(&amount.to_le_bytes());
	
	match withdraw_all {
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

	let withdraw_ix = Instruction {
		program_id: MARGINFI_PROGRAM_ID,
		accounts,
		data: data,
	};

	withdraw_ix
}