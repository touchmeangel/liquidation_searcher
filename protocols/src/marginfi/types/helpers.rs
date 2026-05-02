pub mod price_serde {
  use pyth_solana_receiver_sdk::price_update::Price;
  use serde::{Deserialize, Deserializer, Serialize, Serializer};

  #[derive(Serialize, Deserialize)]
  struct PriceHelper {
      price: i64,
      conf: u64,
      exponent: i32,
      publish_time: i64,
  }

  pub fn serialize<S: Serializer>(p: &Price, s: S) -> Result<S::Ok, S::Error> {
      PriceHelper {
          price: p.price,
          conf: p.conf,
          exponent: p.exponent,
          publish_time: p.publish_time,
      }
      .serialize(s)
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Price, D::Error> {
      let h = PriceHelper::deserialize(d)?;
      Ok(Price {
          price: h.price,
          conf: h.conf,
          exponent: h.exponent,
          publish_time: h.publish_time,
      })
  }
}

pub mod boxed_price_serde {
  use pyth_solana_receiver_sdk::price_update::Price;
  use serde::{Deserializer, Serializer};

  pub fn serialize<S: Serializer>(p: &Box<Price>, s: S) -> Result<S::Ok, S::Error> {
      super::price_serde::serialize(p, s)
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Box<Price>, D::Error> {
      super::price_serde::deserialize(d).map(Box::new)
  }
}

pub mod current_result_serde {
    use switchboard_on_demand::CurrentResult;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    
    #[derive(Serialize, Deserialize)]
    pub struct CurrentResultHelper {
        /// The median value of the submissions needed for quorom size
        pub value: i128,
        /// The standard deviation of the submissions needed for quorom size
        pub std_dev: i128,
        /// The mean of the submissions needed for quorom size
        pub mean: i128,
        /// The range of the submissions needed for quorom size
        pub range: i128,
        /// The minimum value of the submissions needed for quorom size
        pub min_value: i128,
        /// The maximum value of the submissions needed for quorom size
        pub max_value: i128,
        /// The number of samples used to calculate this result
        pub num_samples: u8,
        /// The index of the submission that was used to calculate this result
        pub submission_idx: u8,
        /// Padding bytes for alignment
        pub padding1: [u8; 6],
        /// The slot at which this value was signed.
        pub slot: u64,
        /// The slot at which the first considered submission was made
        pub min_slot: u64,
        /// The slot at which the last considered submission was made
        pub max_slot: u64,
    }

  pub fn serialize<S: Serializer>(p: &CurrentResult, s: S) -> Result<S::Ok, S::Error> {
      CurrentResultHelper {
        value: p.value,
        std_dev: p.std_dev,
        mean: p.mean,
        range: p.range,
        min_value: p.min_value,
        max_value: p.max_value,
        num_samples: p.num_samples,
        submission_idx: p.submission_idx,
        padding1: p.padding1,
        slot: p.slot,
        min_slot: p.min_slot,
        max_slot: p.max_slot,
    }
      .serialize(s)
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<CurrentResult, D::Error> {
      let h = CurrentResultHelper::deserialize(d)?;
      Ok(CurrentResult {
        value: h.value,
        std_dev: h.std_dev,
        mean: h.mean,
        range: h.range,
        min_value: h.min_value,
        max_value: h.max_value,
        num_samples: h.num_samples,
        submission_idx: h.submission_idx,
        padding1: h.padding1,
        slot: h.slot,
        min_slot: h.min_slot,
        max_slot: h.max_slot,
    })
  }
}