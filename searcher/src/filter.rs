use fixed::types::I80F48;
use protocols::marginfi::MarginfiUser;

#[derive(Debug)]
pub struct AccountFilter<T> {
  pub min_asset_value: Option<T>,
  pub max_asset_value: Option<T>,
  pub min_maint_percentage: Option<T>,
  pub max_maint_percentage: Option<T>,
  pub min_maint: Option<T>,
  pub max_maint: Option<T>,
}

impl<T> Default for AccountFilter<T> {
  fn default() -> Self {
    Self {
      min_asset_value: None,
      max_asset_value: None,
      min_maint_percentage: None,
      max_maint_percentage: None,
      min_maint: None,
      max_maint: None,
    }
  }
}

impl<T> AccountFilter<T> where I80F48: PartialOrd<T> {
  // Seized <= Repaid * (1 + max_fee)
  // Where Seized is the equity value withdrawn, in Repaidistheequityvaluerepaid, in,
  // and max fee is the maximum allowed profit currently configured at 10%.
  // Note that equity value is the price of the token without any weights applied, but inclusive of oracle confidence interval adjustments.
  pub fn check(&self, user: &MarginfiUser) -> anyhow::Result<bool> {
    let bank_accounts = user.bank_accounts();
    let asset_value = user.asset_value()?;
    let liability_value = user.liability_value()?;
    let maint = user.maintenance()?;
    let maint_percentage = maint.checked_div(asset_value).unwrap_or(I80F48::from_num(1));
    
    if let Some(ref min) = self.min_asset_value
      && &asset_value < min {
        return Ok(false);
      }
    
    if let Some(ref max) = self.max_asset_value
      && &asset_value > max {
        return Ok(false);
      }

    if let Some(ref min) = self.min_maint_percentage
      && &maint_percentage < min {
        return Ok(false);
      }

    if let Some(ref max) = self.max_maint_percentage
      && &maint_percentage >= max {
        return Ok(false);
      }
    
    if let Some(ref min) = self.min_maint
      && &maint < min {
        return Ok(false);
      }
    
    if let Some(ref max) = self.max_maint
      && &maint > max {
        return Ok(false);
      }
    
    Ok(true)
  }    
}