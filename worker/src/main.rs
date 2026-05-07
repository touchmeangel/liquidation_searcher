mod config;

use std::{collections::{HashMap, HashSet}, sync::Arc};

use config::Config;
use connections::{SubRedis, queue_keys};
use fixed::types::I80F48;
use jupiter_swap_api_client::build::BuildInstructionsResponse;
use protocols::marginfi::{BalanceSide, BankAccount, FeeState, Marginfi, MarginfiUser};
use solana_account::Account;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;
use solana_sdk::{message::{AddressLookupTableAccount, VersionedMessage, v0}, signature::Keypair, signer::Signer, transaction::VersionedTransaction};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use tokio::{signal, sync::Semaphore};

#[tokio::main]
async fn main() {
  let config = Config::open().unwrap();

  let result = start(config).await;

  if let Err(err) = result {
    eprintln!("error: {err}");
    
    err.chain()
      .skip(1)
      .for_each(|cause| eprintln!("caused by:\n  {cause}"));
  }
}

async fn start(config: Config) -> anyhow::Result<()> {
  let marginfi = Arc::new(Marginfi::new(config.http_url.clone(), config.ws_url.clone()).await?);
  let fee_state = Arc::new(marginfi.get_fee_state().await?);
  let liquidation_max_fee: I80F48 = fee_state.liquidation_max_fee.into();
  let liquidation_flat_sol_fee: I80F48 = fee_state.liquidation_flat_sol_fee.into();
	if liquidation_flat_sol_fee < config.safety_margin - 1.0 {
  	println!("SAFETY_MARGIN is to high, for correct calculations it must be lower than liquidation_max_fee = {}%", liquidation_max_fee.checked_mul(I80F48::from_num(100)).unwrap_or(I80F48::ZERO));
		return Ok(());
	}
  println!("FeeState is currently defined as liquidation_max_fee = {}% liquidation_flat_sol_fee = {} SOL", liquidation_max_fee.checked_mul(I80F48::from_num(100)).unwrap_or(I80F48::ZERO), liquidation_flat_sol_fee.checked_div(I80F48::from_num(1_000_000_000)).unwrap_or(I80F48::ZERO));

  let mut subredis = SubRedis::new(&config.pubsub_url).await?;
  println!("connection established, listening");

  let semaphore = Arc::new(Semaphore::new(config.capacity));

  loop {
    tokio::select! {
      result = subredis.builder::<MarginfiUser>(queue_keys::LIQUIDATION_QUEUE, 1).recv() => {
        let mut accounts = match result {
          Ok(messages) => messages,
          Err(err) => {
            println!("error while reading: {}", err);
            continue
          },
        };
        
        let result = match accounts.pop() {
          Some(account) => account,
          None => continue,
        };
        let (pubkey, account) = match result {
          Ok((pubkey, account)) => (pubkey, account),
          Err(err) => {
            println!("error parsing arguments: {}", err);
            continue
          },
        };
        
        let permit = semaphore.clone();
        let config_clone = config.clone();
        let marginfi_clone = Arc::clone(&marginfi);
        let fee_state_clone = Arc::clone(&fee_state);
        tokio::spawn(async move {
          let _guard = permit.acquire().await.unwrap();

          if let Err(err) = handle(config_clone, &marginfi_clone, &fee_state_clone, pubkey, account).await {
            println!("error liquidating accounts: {}", err);
          };
        });
      }
      _ = signal::ctrl_c() => {
        println!("shutting down");
        break;
      }
    }
  }

  Ok(())
}

async fn handle(config: Config, marginfi: &Marginfi, fee_state: &FeeState, pubkey: Pubkey, account: MarginfiUser) -> anyhow::Result<()> {
  println!("RECEIVED {}", pubkey);
  let withdrawable_assets = account.withdrawable_asset_value()?;
	let liability = account.liability_value()?;
	let liability_with_safety = liability.checked_mul(I80F48::from_num(config.safety_margin))
		.ok_or(anyhow::anyhow!("Math error at {}", line!()))?;

	if withdrawable_assets.checked_sub(liability_with_safety)
		.ok_or(anyhow::anyhow!("Math error at {}", line!()))? <= 0 {
		println!("{} is deep in debt, not profitable to liquidate, consider lowering SAFETY_MARGIN if its to high ({})", pubkey, config.safety_margin);
    return anyhow::Ok(());
  }
  
	let seizable = withdrawable_assets.checked_sub(liability).ok_or(anyhow::anyhow!("Math error at {}", line!()))?;
  println!("{}$ to make, max {}$ (w: {}, l: {})", seizable, liability.checked_mul(fee_state.liquidation_max_fee.into()).unwrap_or(I80F48::ZERO), withdrawable_assets, liability);

	let swaps = calculate_swap_pairs(&account, config.safety_margin)?;
	let max_assets = liability
		+ liability
			.checked_mul(fee_state.liquidation_max_fee.into())
			.ok_or(anyhow::anyhow!("Math error at {}", line!()))?;

	let haircut = I80F48::from_num(config.asset_haircut);

	let assets_needed = max_assets
		.checked_mul(haircut)
		.ok_or(anyhow::anyhow!("Math error at {}", line!()))?;

	let assets_to_withdraw = select_assets_to_withdraw(&account, &swaps, assets_needed)?;

	// let mint_pubkeys: Vec<Pubkey> = assets_to_withdraw.iter()
	// 	.map(|a| a.mint)
	// 	.collect();
    
	// let mint_accounts = rpc.get_multiple_accounts(&mint_pubkeys).await?;

  // 3VzSmqcYQaKcA8vFoqW5batNPNWVvqpVXtFmKHse7SUE
  // AiC3orMdwW2hG9Xhv53nktgDwq4cLkqLAfMcNQFoXWoJ
  // 2qD4c8Z4kFM8s629igaw9Rbc2DGx67bS2w2VawVAwaLd
  // 3GsZWEBFuoe1ooX8PiFASQPnfCeDZNdY8cCFB8ZESHfT
  // GnSNK7gpepE1PRZFSYCNr1KrBZXf8mBMiZzjn54forzw

  // DKTZBDzCgFcHu8QhjQCupo2GRiKbDWLmAYxwVUn38S5J
  // LENDING:
  // JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN: 47.69020247742894$
  // 27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4: 9.883730130318696$
  // BORROWING:
  // susdabGDNbhrnCa6ncrYo81u4s9GM8ecK2UwMyZiq4X: 51.69141136818984$

  Ok(())
}

fn build_available_assets_map(user: &MarginfiUser) -> HashMap<Pubkey, AssetNode> {
	let bank_accounts = user.bank_accounts();
	let available: HashMap<Pubkey, AssetNode> = bank_accounts
		.iter()
		.filter(|b| !b.balance.is_empty(BalanceSide::Assets) || !user.is_bank_withdrawable(*b))
		.filter_map(|b| 
			Some(
				(b.bank.mint.clone(), AssetNode {
					bank: b.clone(),
					amount: b.balance.asset_shares.into(),
					usd_value: b.asset_value().ok()?
				})
			)
		)
		.collect();

	available
}

#[derive(Clone)]
pub struct AssetToWithdraw {
	pub mint: Pubkey,
	pub amount: I80F48,
	pub amount_usd: I80F48,
	pub bank: BankAccount
}

pub fn select_assets_to_withdraw(
	user: &MarginfiUser,
	swaps: &[SwapPair],
	target_usd: I80F48,
) -> anyhow::Result<Vec<AssetToWithdraw>> {
	let available = build_available_assets_map(&user);
	let mut swap_totals: HashMap<Pubkey, (I80F48, I80F48)> = HashMap::new();
	
	for swap in swaps {
		let entry = swap_totals.entry(swap.from_mint).or_insert((I80F48::ZERO, I80F48::ZERO));
		entry.0 = entry.0.checked_add(swap.from_amount)
			.ok_or(anyhow::anyhow!("Math error: amount overflow"))?;
		entry.1 = entry.1.checked_add(swap.from_amount_usd)
			.ok_or(anyhow::anyhow!("Math error: USD overflow"))?;
	}
	
	let mut candidates: Vec<AssetToWithdraw> = Vec::new();
	for (mint, (total_amount, total_usd)) in swap_totals {
		let asset_node = available.get(&mint)
			.ok_or(anyhow::anyhow!("Asset {} not found in available balances", mint))?;
		
		if total_usd > asset_node.usd_value {
			return Err(anyhow::anyhow!(
				"Swap requires {} USD of {}, but only {} USD available",
				total_usd,
				mint,
				asset_node.usd_value
			));
		}
		
		candidates.push(AssetToWithdraw {
			bank: asset_node.bank.clone(),
			mint,
			amount: total_amount,
			amount_usd: total_usd,
		});
	}
	
	candidates.sort_by(|a, b| b.amount_usd.cmp(&a.amount_usd));
	let mut selected = Vec::new();
	let mut accumulated_usd = I80F48::ZERO;
	
	for mut candidate in candidates {
		let remaining_needed = target_usd.checked_sub(accumulated_usd)
			.ok_or(anyhow::anyhow!("Math error: subtraction overflow"))?;
		
		if remaining_needed <= I80F48::ZERO {
			break;
		}
		
		let asset_node = available.get(&candidate.mint).unwrap(); // Safe: we validated earlier
		let max_additional_usd = asset_node.usd_value.checked_sub(candidate.amount_usd)
			.ok_or(anyhow::anyhow!("Math error: max additional overflow"))?;
		
		if max_additional_usd > I80F48::ZERO && remaining_needed > candidate.amount_usd {
			let additional_usd = max_additional_usd.min(remaining_needed - candidate.amount_usd);
				
			let price = candidate.amount_usd.checked_div(candidate.amount)
				.ok_or(anyhow::anyhow!("Math error: price calculation"))?;
			let additional_amount = additional_usd.checked_div(price)
				.ok_or(anyhow::anyhow!("Math error: amount calculation"))?;
			
			candidate.amount = candidate.amount.checked_add(additional_amount)
				.ok_or(anyhow::anyhow!("Math error: amount overflow"))?;
			candidate.amount_usd = candidate.amount_usd.checked_add(additional_usd)
				.ok_or(anyhow::anyhow!("Math error: USD overflow"))?;
		}
		
		accumulated_usd = accumulated_usd.checked_add(candidate.amount_usd)
			.ok_or(anyhow::anyhow!("Math error: accumulated USD overflow"))?;
		selected.push(candidate);
	}
	
	Ok(selected)
}

#[derive(Clone)]
pub struct AssetNode {
	pub bank: BankAccount,
	pub amount: I80F48,
	pub usd_value: I80F48
}

#[derive(Clone)]
pub struct SwapPair {
	pub from_mint: Pubkey,
	pub to_mint: Pubkey,
	pub from_amount: I80F48,
	pub from_amount_usd: I80F48
}

pub fn calculate_swap_pairs(user: &MarginfiUser, safety_margin: f64) -> anyhow::Result<Vec<SwapPair>> {
	let mut available = build_available_assets_map(&user);
	let bank_accounts = user.bank_accounts();

	let needed: HashMap<Pubkey, AssetNode> = bank_accounts
		.iter()
		.filter(|b| !b.balance.is_empty(BalanceSide::Liabilities))
		.filter_map(|b| 
			Some(
				(b.bank.mint.clone(), AssetNode {
					bank: b.clone(),
					amount: b.balance.liability_shares.into(),
					usd_value: b.liability_value().ok()?
				})
			)
		)
		.collect();

	let mut swaps = Vec::new();

	for (mint, needed_bank) in needed.iter() {
		if let Some(available_bank) = available.get_mut(mint) {
			let amount_to_use = needed_bank.amount.min(available_bank.amount);
			
			if amount_to_use > 0.0 {
				let unit_price = available_bank.usd_value / available_bank.amount;
				
				swaps.push(SwapPair {
					from_mint: mint.clone(),
					to_mint: mint.clone(),
					from_amount: amount_to_use,
					from_amount_usd: amount_to_use * unit_price
				});
				
				available_bank.amount -= amount_to_use;
				available_bank.usd_value = available_bank.amount * unit_price;
			}
		}
	}

	for (needed_asset_mint, needed_asset) in needed.iter() {
		let already_covered = swaps.iter()
			.filter(|s| s.to_mint == *needed_asset_mint && s.from_mint == *needed_asset_mint)
			.map(|s| s.from_amount)
			.sum::<I80F48>();
		
		let remaining_needed_amount = needed_asset.amount - already_covered;
		
		if remaining_needed_amount <= 0.0 {
			continue;
		}
		
		let unit_price = if needed_asset.amount > 0.0 {
			needed_asset.usd_value / needed_asset.amount
		} else {
			continue;
		};
		let usd_value_needed = remaining_needed_amount * unit_price * I80F48::from_num(safety_margin);
		
		let mut sorted_available: Vec<_> = available
			.iter()
			.filter(|(k, v)| *k != needed_asset_mint && v.usd_value > 0.0)
			.map(|(k, v)| (k.clone(), v.clone()))
			.collect();
		sorted_available.sort_by(|a, b| b.1.usd_value.partial_cmp(&a.1.usd_value).unwrap());
		
		let mut remaining_value = usd_value_needed;
		
		for (available_asset_mint, _) in sorted_available {
			if remaining_value <= 0.0 {
				break;
			}
			
			let available_asset = available.get_mut(&available_asset_mint).unwrap();
			
			if available_asset.usd_value <= 0.0 || available_asset.amount <= 0.0 {
				continue;
			}
			
			let value_to_use = remaining_value.min(available_asset.usd_value);
			let unit_price = available_asset.usd_value / available_asset.amount;
			let amount_to_swap = value_to_use / unit_price;
			
			swaps.push(SwapPair {
				from_mint: available_asset_mint,
				to_mint: needed_asset_mint.clone(),
				from_amount: amount_to_swap,
				from_amount_usd: amount_to_swap * unit_price
			});
			
			available_asset.amount -= amount_to_swap;
			available_asset.usd_value -= value_to_use;
			remaining_value -= value_to_use;
		}
		
		if remaining_value > 0.01 {
			anyhow::bail!(
				"Insufficient funds to cover {} (${:.2} short)",
				needed_asset_mint, remaining_value
			);
		}
	}

	Ok(Vec::new())
}

pub async fn build_liquidation_tx(
  rpc_client: &RpcClient,
	user: &MarginfiUser,
	fee_state: &FeeState,
  payer: &Keypair,
	assets_to_withdraw: Vec<(AssetToWithdraw, Account)>,
  swap_responses: Vec<BuildInstructionsResponse>,
) -> anyhow::Result<()> {
	let payer_pubkey = payer.pubkey();

  let (cu_price_ix, _) = swap_responses
    .iter()
    .flat_map(|s| s.compute_budget_instructions.iter())
    .filter(|ix| {
			ix.program_id == solana_compute_budget_interface::ID
				&& ix.data.first() == Some(&3u8)
    })
    .filter_map(|ix| {
			if ix.data.len() >= 9 {
				Some((ix, u64::from_le_bytes(ix.data[1..9].try_into().ok()?)))
			} else {
				None
			}
    })
    .max_by(|(_, cu_price_a), (_, cu_price_b)| Ord::cmp(cu_price_a, cu_price_b))
    .unzip();

  let lookup_tables: Vec<AddressLookupTableAccount> = swap_responses
		.iter()
		.flat_map(|s| {
			s.addresses_by_lookup_table_address
				.clone()
				.unwrap_or_default()
				.into_iter()
		})
		.fold(HashMap::new(), |mut map, (key, addresses)| {
			map.entry(key).or_insert(addresses);
			map
		})
		.into_iter()
		.map(|(key, addresses)| AddressLookupTableAccount { key, addresses })
		.collect();

  let swap_instructions = build_liquidation_instructions(
		user,
		payer,
		&swap_responses,
		cu_price_ix.map(|ix| ix.clone()),
		assets_to_withdraw,
		fee_state.global_fee_wallet
	);

  let blockhash = rpc_client.get_latest_blockhash().await?;

  let sim_instructions: Vec<Instruction> = std::iter::once(
    ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
  )
  .chain(swap_instructions.clone())
  .collect();

  let sim_msg = v0::Message::try_compile(
		&payer_pubkey,
		&sim_instructions,
		&lookup_tables,
		blockhash,
  )?;

  let sim_tx = VersionedTransaction::try_new(
		VersionedMessage::V0(sim_msg),
		&[payer],
  )?;

  let sim_result = rpc_client
		.simulate_transaction_with_config(
			&sim_tx,
			RpcSimulateTransactionConfig {
				replace_recent_blockhash: true, // don't need a fresh blockhash just for sim
				commitment: Some(rpc_client.commitment()),
				..Default::default()
			},
		)
		.await?;

  if let Some(err) = sim_result.value.err {
    anyhow::bail!("simulation failed: {err:?}\nlogs: {:#?}", sim_result.value.logs);
  }

  let cu_consumed = sim_result
		.value
		.units_consumed
		.ok_or_else(|| anyhow::anyhow!("simulation returned no units_consumed"))?;

  println!("cu_limit: {}", cu_consumed);

  Ok(())
}

fn build_liquidation_instructions(
	user: &MarginfiUser,
	payer: &Keypair,
  swap_responses: &[BuildInstructionsResponse],
  cu_price_ix: Option<Instruction>,
	assets_to_withdraw: Vec<(AssetToWithdraw, Account)>,
	global_fee_wallet: Pubkey
) -> Vec<Instruction> {
  let mut instructions = Vec::new();

  if let Some(ix) = cu_price_ix {
		instructions.push(ix);
  }

	instructions.push(user.start_liquidation_ix(payer.pubkey()));

	for (asset, mint_account) in assets_to_withdraw {
		let token_program = mint_account.owner;
		
		let destination_token_account = get_associated_token_address_with_program_id(
			&payer.pubkey(),
			&asset.mint,
			&token_program,
		);
		
		instructions.push(
			spl_associated_token_account::instruction::create_associated_token_account_idempotent(
				payer,
				payer,
				&asset.mint,
				&token_program,
			)
		);

		instructions.push(
			user.withdraw_ix(
				payer.pubkey(),
				&asset.bank,
				destination_token_account,
				token_program,
				asset.amount,
				Some(false)
			)
		);
	}

  let dedup_key = |ix: &Instruction| {
		let writable: Vec<Pubkey> = ix.accounts
			.iter()
			.filter(|a| a.is_writable)
			.map(|a| a.pubkey)
			.collect();
		(ix.program_id, writable)
  };

  let mut seen_setup = HashSet::new();
  for swap in swap_responses {
		for ix in &swap.setup_instructions {
			if seen_setup.insert(dedup_key(ix)) {
				instructions.push(ix.clone());
			} else {
				continue;
			}
		}
  }

  for swap in swap_responses {
		instructions.push(swap.swap_instruction.clone());
  }

  let mut seen_cleanup = HashSet::new();
  for swap in swap_responses {
		if let Some(ix) = &swap.cleanup_instruction {
			if seen_cleanup.insert(dedup_key(ix)) {
				instructions.push(ix.clone());
			} else {
				continue;
			}
		}
		instructions.extend(swap.other_instructions.clone());
		if let Some(ix) = &swap.tip_instruction {
			instructions.push(ix.clone());
		}
  }

	instructions.push(user.end_liquidation_ix(payer.pubkey(), global_fee_wallet));

  instructions
}

// async fn liquidate()