//! SEP-40 price feed interface.
//!
//! The trait signature below is copied verbatim from
//! <https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0040.md>.
//! Per the spec, the read functions never throw for unknown assets or
//! out-of-range timestamps; they return `None` so consumers can handle the
//! error themselves.

use crate::admin::read_commodities;
use crate::ingest::{read_history, round_to_window};
use crate::types::{Asset, DataKey, PriceData};
use crate::{Contract, ContractArgs, ContractClient};
use soroban_sdk::{contractimpl, contracttrait, Env, Symbol, Vec};

/// The SEP-40 oracle consumer interface.
#[contracttrait]
pub trait PriceFeedTrait {
    /// Return the base asset the price is reported in.
    fn base(env: Env) -> Asset;
    /// Return all assets quoted by the price feed.
    fn assets(env: Env) -> Vec<Asset>;
    /// Return the number of decimals for all assets quoted by the oracle.
    fn decimals(env: Env) -> u32;
    /// Return default tick period timeframe (in seconds).
    fn resolution(env: Env) -> u32;
    /// Get price in base asset at a specific timestamp.
    fn price(env: Env, asset: Asset, timestamp: u64) -> Option<PriceData>;
    /// Get the last `records` price points for an asset.
    fn prices(env: Env, asset: Asset, records: u32) -> Option<Vec<PriceData>>;
    /// Get the most recent price for an asset.
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData>;
}

#[contractimpl]
impl PriceFeedTrait for Contract {
    /// Returns the base asset the price is reported in.
    ///
    /// For an uninitialized contract the stored value is absent, so a
    /// degenerate empty-symbol asset is returned; consumers should treat an
    /// uninitialized feed as misconfigured.
    fn base(env: Env) -> Asset {
        env.storage()
            .instance()
            .get(&DataKey::BaseAsset)
            .unwrap_or_else(|| Asset::Other(Symbol::new(&env, "")))
    }

    /// Returns all commodities tracked by the feed. Empty for an
    /// uninitialized contract.
    fn assets(env: Env) -> Vec<Asset> {
        read_commodities(&env)
    }

    /// Returns the number of decimals used to represent prices. Zero for an
    /// uninitialized contract.
    fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Decimals)
            .unwrap_or(0)
    }

    /// Returns the resolution window in seconds. Zero for an uninitialized
    /// contract.
    fn resolution(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Resolution)
            .unwrap_or(0)
    }

    /// Returns the price for `asset` at the given point in time, or `None`
    /// when the asset is unknown or the queried window has no data.
    ///
    /// The queried timestamp is rounded down to the feed's resolution window
    /// before matching, so any timestamp inside a finalized window returns
    /// that window's price. Returns `None` for unknown assets and for
    /// timestamps whose window has no finalized data.
    fn price(env: Env, asset: Asset, timestamp: u64) -> Option<PriceData> {
        let history = read_history(&env, &asset);
        let target = round_to_window(&env, timestamp);
        for record in history.iter() {
            if record.timestamp == target {
                return Some(record);
            }
        }
        None
    }

    /// Returns up to `records` most recent price points for `asset`, ordered
    /// oldest first so consumers can iterate chronologically.
    ///
    /// Returns `None` when `records` is zero or when the asset has no history
    /// (unknown asset or nothing finalized yet). Fewer than `records` entries
    /// are returned when the history is shorter.
    fn prices(env: Env, asset: Asset, records: u32) -> Option<Vec<PriceData>> {
        if records == 0 {
            return None;
        }
        let history = read_history(&env, &asset);
        if history.is_empty() {
            return None;
        }
        let take = if records < history.len() {
            records
        } else {
            history.len()
        };
        Some(history.slice((history.len() - take)..))
    }

    /// Returns the most recent price for `asset`, or `None` when the asset is
    /// unknown or has no finalized history.
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        read_history(&env, &asset).last()
    }
}