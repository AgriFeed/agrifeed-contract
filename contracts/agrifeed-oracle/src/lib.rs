//! agrifeed-oracle
//!
//! A SEP-40 compliant price oracle for agricultural commodity prices
//! (cocoa, coffee, cashew, cotton, maize). Multiple authorized nodes submit
//! prices per commodity, and a permissionless finalize step aggregates the
//! pending submissions into a single median price per resolution window.
#![no_std]

mod admin;
mod errors;
mod storage;
mod types;

pub use errors::Error;
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

#[contract]
pub struct Contract;