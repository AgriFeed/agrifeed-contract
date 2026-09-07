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

#[contract]
pub struct Contract;