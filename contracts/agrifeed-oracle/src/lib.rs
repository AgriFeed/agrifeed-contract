//! agrifeed-oracle
//!
//! A SEP-40 compliant price oracle for agricultural commodity prices
//! (cocoa, coffee, cashew, cotton, maize). Multiple authorized nodes submit
//! prices per commodity, and a permissionless finalize step aggregates the
//! pending submissions into a single median price per resolution window.
#![no_std]
// Public function signatures are fixed by the application spec (for example
// initialize takes eight arguments), so the lint is allowed at crate level.
#![allow(clippy::too_many_arguments)]

mod admin;
mod errors;
mod ingest;
mod interface;
mod storage;
#[cfg(test)]
mod test;
mod types;

pub use errors::Error;
pub use interface::PriceFeedTrait;
pub use types::{Asset, DataKey, PriceData, Submission};

use soroban_sdk::{contract, contractevent, Address};

/// Default maximum number of finalized price records kept per commodity.
pub const DEFAULT_RETENTION: u32 = 90;

/// Emitted by [`Contract::initialize`] with the initial configuration.
#[contractevent]
pub struct Initialized {
    #[topic]
    pub admin: Address,
    pub base_asset: Asset,
    pub decimals: u32,
    pub resolution: u32,
}

/// Emitted by [`Contract::add_node`] when a new price node is added.
#[contractevent]
pub struct NodeAdded {
    #[topic]
    pub node: Address,
}

/// Emitted by [`Contract::remove_node`] when a price node is removed.
#[contractevent]
pub struct NodeRemoved {
    #[topic]
    pub node: Address,
}

/// Emitted by [`Contract::set_threshold`] when the finalize threshold changes.
#[contractevent]
pub struct ThresholdUpdated {
    #[topic]
    pub threshold: u32,
}

/// Emitted by [`Contract::set_retention`] when the history retention limit
/// changes.
#[contractevent]
pub struct RetentionUpdated {
    #[topic]
    pub retention: u32,
}

/// Emitted by [`Contract::add_commodity`] when a new commodity is tracked.
#[contractevent]
pub struct CommodityAdded {
    #[topic]
    pub asset: Asset,
}

/// Emitted by [`Contract::submit_price`] when a node submits an observation.
#[contractevent]
pub struct PriceSubmitted {
    #[topic]
    pub asset: Asset,
    #[topic]
    pub node: Address,
    pub price: i128,
}

/// Emitted by [`Contract::finalize_price`] with the finalized median price.
#[contractevent]
pub struct PriceFinalized {
    #[topic]
    pub asset: Asset,
    pub price: i128,
    pub timestamp: u64,
}

#[contract]
pub struct Contract;
