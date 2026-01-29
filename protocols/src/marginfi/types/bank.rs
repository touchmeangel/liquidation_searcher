use crate::{
  assert_struct_align, assert_struct_size,
};

use bytemuck::{Pod, Zeroable};

use solana_pubkey::Pubkey;

use fixed::types::I80F48;

use super::{BankCache, BankConfig, EmodeSettings};
use super::super::consts::discriminators;
use super::super::WrappedI80F48;

assert_struct_size!(Bank, 1856);
assert_struct_align!(Bank, 8);
#[repr(C)]
#[derive(Pod, Zeroable)]
#[derive(Debug, Clone, Copy)]
pub struct Bank {
  pub mint: Pubkey,
  pub mint_decimals: u8,

  pub group: Pubkey,

  // Note: The padding is here, not after mint_decimals. Pubkey has alignment 1, so those 32
  // bytes can cross the alignment 8 threshold, but WrappedI80F48 has alignment 8 and cannot
  pub _pad0: [u8; 7], // 1x u8 + 7 = 8

  /// Monotonically increases as interest rate accumulates. For typical banks, a user's asset
  /// value in token = (number of shares the user has * asset_share_value).
  /// * A float (arbitrary decimals)
  /// * Initially 1
  pub asset_share_value: WrappedI80F48,
  /// Monotonically increases as interest rate accumulates. For typical banks, a user's liabilty
  /// value in token = (number of shares the user has * liability_share_value)
  /// * A float (arbitrary decimals)
  /// * Initially 1
  pub liability_share_value: WrappedI80F48,

  pub liquidity_vault: Pubkey,
  pub liquidity_vault_bump: u8,
  pub liquidity_vault_authority_bump: u8,

  pub insurance_vault: Pubkey,
  pub insurance_vault_bump: u8,
  pub insurance_vault_authority_bump: u8,

  pub _pad1: [u8; 4], // 4x u8 + 4 = 8

  /// Fees collected and pending withdraw for the `insurance_vault`
  pub collected_insurance_fees_outstanding: WrappedI80F48,

  pub fee_vault: Pubkey,
  pub fee_vault_bump: u8,
  pub fee_vault_authority_bump: u8,

  pub _pad2: [u8; 6], // 2x u8 + 6 = 8

  /// Fees collected and pending withdraw for the `fee_vault`
  pub collected_group_fees_outstanding: WrappedI80F48,

  /// Sum of all liability shares held by all borrowers in this bank.
  /// * Uses `mint_decimals`
  pub total_liability_shares: WrappedI80F48,
  /// Sum of all asset shares held by all depositors in this bank.
  /// * Uses `mint_decimals`
  /// * For Kamino banks, this is the quantity of collateral tokens (NOT liquidity tokens) in the
  ///   bank, and also uses `mint_decimals`, though the mint itself will always show (6) decimals
  ///   exactly (i.e Kamino ignores this and treats it as if it was using `mint_decimals`)
  pub total_asset_shares: WrappedI80F48,

  pub last_update: i64,

  pub config: BankConfig,

  /// Bank Config Flags
  ///
  /// - EMISSIONS_FLAG_BORROW_ACTIVE: 1
  /// - EMISSIONS_FLAG_LENDING_ACTIVE: 2
  /// - PERMISSIONLESS_BAD_DEBT_SETTLEMENT: 4
  /// - FREEZE_SETTINGS: 8
  /// - CLOSE_ENABLED_FLAG: 16
  /// - TOKENLESS_REPAYMENTS_ACTIVE: 32
  ///
  pub flags: u64,
  /// Emissions APR. Number of emitted tokens (emissions_mint) per 1e(bank.mint_decimal) tokens
  /// (bank mint) (native amount) per 1 YEAR.
  pub emissions_rate: u64,
  pub emissions_remaining: WrappedI80F48,
  pub emissions_mint: Pubkey,

  /// Fees collected and pending withdraw for the `FeeState.global_fee_wallet`'s canonical ATA for `mint`
  pub collected_program_fees_outstanding: WrappedI80F48,

  /// Controls this bank's emode configuration, which enables some banks to treat the assets of
  /// certain other banks more preferentially as collateral.
  pub emode: EmodeSettings,

  /// Set with `update_fees_destination_account`. Fees can be withdrawn to the canonical ATA of
  /// this wallet without the admin's input (withdraw_fees_permissionless). If pubkey default, the
  /// bank doesn't support this feature, and the fees must be collected manually (withdraw_fees).
  pub fees_destination_account: Pubkey,

  pub cache: BankCache,
  /// Number of user lending positions currently open in this bank
  /// * For banks created prior to 0.1.4, this is the number of positions opened/closed after
  ///   0.1.4 goes live, and may be negative.
  /// * For banks created in 0.1.4 or later, this is the number of positions open in total, and
  ///   the bank may safely be closed if this is zero. Will never go negative.
  pub lending_position_count: i32,
  /// Number of user borrowing positions currently open in this bank
  /// * For banks created prior to 0.1.4, this is the number of positions opened/closed after
  ///   0.1.4 goes live, and may be negative.
  /// * For banks created in 0.1.4 or later, this is the number of positions open in total, and
  ///   the bank may safely be closed if this is zero. Will never go negative.
  pub borrowing_position_count: i32,

  pub _padding_0: [u8; 16],

  /// Kamino banks only, otherwise Pubkey default
  pub kamino_reserve: Pubkey,
  /// Kamino banks only, otherwise Pubkey default
  pub kamino_obligation: Pubkey,

  pub _padding_1: [[u64; 2]; 15], // 8 * 2 * 14 = 224B
}

impl Bank {
  pub const LEN: usize = std::mem::size_of::<Bank>();
  pub const DISCRIMINATOR: [u8; 8] = discriminators::BANK;

  pub fn get_liability_amount(&self, shares: I80F48) -> Option<I80F48> {
    shares
        .checked_mul(self.liability_share_value.into())
  }

  pub fn get_asset_amount(&self, shares: I80F48) -> Option<I80F48> {
    shares
        .checked_mul(self.asset_share_value.into())
  }

  pub fn get_display_asset(&self, amount: I80F48) -> Option<I80F48> {
    let div = I80F48::from_num(10_i128.pow(self.mint_decimals as u32));
    amount
      .checked_div(div)
  }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub struct RawRiskTier(pub u8);

impl RawRiskTier {
  pub fn validate(&self) -> Result<RiskTier, String> {
      match self.0 {
          0 => Ok(RiskTier::Collateral),
          1 => Ok(RiskTier::Isolated),
          _ => Err(format!("Invalid RiskTier: {}", self.0)),
      }
  }
}
unsafe impl Zeroable for RawRiskTier {}
unsafe impl Pod for RawRiskTier {}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub enum RiskTier {
  #[default]
  Collateral = 0,
  /// ## Isolated Risk
  /// Assets in this trance can be borrowed only in isolation.
  /// They can't be borrowed together with other assets.
  ///
  /// For example, if users has USDC, and wants to borrow XYZ which is isolated,
  /// they can't borrow XYZ together with SOL, only XYZ alone.
  Isolated = 1,
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct RawBankOperationalState(pub u8);

impl RawBankOperationalState {
  pub fn validate(&self) -> Result<BankOperationalState, String> {
      match self.0 {
          0 => Ok(BankOperationalState::Paused),
          1 => Ok(BankOperationalState::Operational),
          2 => Ok(BankOperationalState::ReduceOnly),
          3 => Ok(BankOperationalState::KilledByBankruptcy),
          _ => Err(format!("Invalid BankOperationalState: {}", self.0)),
      }
  }
}
unsafe impl Zeroable for RawBankOperationalState {}
unsafe impl Pod for RawBankOperationalState {}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum BankOperationalState {
  Paused,
  Operational,
  ReduceOnly,
  KilledByBankruptcy,
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub struct RawOracleSetup(pub u8);

impl RawOracleSetup {
    pub fn validate(&self) -> Result<OracleSetup, String> {
        match self.0 {
            0 => Ok(OracleSetup::None),
            1 => Ok(OracleSetup::PythLegacy),
            2 => Ok(OracleSetup::SwitchboardV2),
            3 => Ok(OracleSetup::PythPushOracle),
            4 => Ok(OracleSetup::SwitchboardPull),
            5 => Ok(OracleSetup::StakedWithPythPush),
            6 => Ok(OracleSetup::KaminoPythPush),
            7 => Ok(OracleSetup::KaminoSwitchboardPull),
            8 => Ok(OracleSetup::Fixed),
            9 => Ok(OracleSetup::DriftPythPull),
            10 => Ok(OracleSetup::DriftSwitchboardPull),
            11 => Ok(OracleSetup::SolendPythPull),
            12 => Ok(OracleSetup::SolendSwitchboardPull),
            _ => Err(format!("Invalid OracleSetup: {}", self.0)),
        }
    }
}
unsafe impl Zeroable for RawOracleSetup {}
unsafe impl Pod for RawOracleSetup {}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum OracleSetup {
  None,
  PythLegacy,
  SwitchboardV2,
  PythPushOracle,
  SwitchboardPull,
  StakedWithPythPush,
  KaminoPythPush,
  KaminoSwitchboardPull,
  Fixed,
  DriftPythPull,
  DriftSwitchboardPull,
  SolendPythPull,
  SolendSwitchboardPull,
}
unsafe impl Zeroable for OracleSetup {}
unsafe impl Pod for OracleSetup {}

impl OracleSetup {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::PythLegacy),    // Deprecated
            2 => Some(Self::SwitchboardV2), // Deprecated
            3 => Some(Self::PythPushOracle),
            4 => Some(Self::SwitchboardPull),
            5 => Some(Self::StakedWithPythPush),
            6 => Some(Self::KaminoPythPush),
            7 => Some(Self::KaminoSwitchboardPull),
            8 => Some(Self::Fixed),
            9 => Some(Self::DriftPythPull),
            10 => Some(Self::DriftSwitchboardPull),
            11 => Some(Self::SolendPythPull),
            12 => Some(Self::SolendSwitchboardPull),
            _ => None,
        }
    }
}