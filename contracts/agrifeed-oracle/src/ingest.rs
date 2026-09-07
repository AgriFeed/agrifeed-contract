//! Price ingestion: node submissions and permissionless finalization.

use crate::admin::{read_commodities, read_nodes, require_initialized};
use crate::errors::Error;
use crate::storage;
use crate::types::{Asset, DataKey, PriceData, Submission};
use crate::{Contract, ContractArgs, ContractClient, PriceFinalized, PriceSubmitted};
use soroban_sdk::{contractimpl, Address, Env, Vec};

/// Reads the pending submissions for an asset.
pub(crate) fn read_pending(env: &Env, asset: &Asset) -> Vec<Submission> {
    env.storage()
        .persistent()
        .get(&DataKey::Pending(asset.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

/// Rounds a timestamp down to the contract's resolution window, per the
/// SEP-40 design rationale: `floor(ts / resolution) * resolution`.
///
/// If the resolution is not configured yet, the timestamp is returned
/// unchanged; callers that require rounding check initialization first.
/// Reads the finalized price history for an asset.
pub(crate) fn read_history(env: &Env, asset: &Asset) -> Vec<PriceData> {
    env.storage()
        .persistent()
        .get(&DataKey::History(asset.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

/// Sorts the pending submission prices in ascending order using insertion
/// sort.
///
/// All indices are guarded by the loop bounds (`i < len`, `j <= i`), so
/// `get_unchecked` is only ever called with in-bounds positions.
fn sorted_prices(env: &Env, pending: &Vec<Submission>) -> Vec<i128> {
    let mut prices: Vec<i128> = Vec::new(env);
    for sub in pending.iter() {
        prices.push_back(sub.price);
    }
    let n = prices.len();
    let mut i: u32 = 1;
    while i < n {
        let key = prices.get_unchecked(i);
        let mut j = i;
        while j > 0 {
            let prev = prices.get_unchecked(j - 1);
            if prev <= key {
                break;
            }
            prices.set(j, prev);
            j -= 1;
        }
        prices.set(j, key);
        i += 1;
    }
    prices
}

/// Computes the integer median of the pending submission prices.
///
/// For an odd count the middle value is returned. For an even count the
/// average of the two middle values is returned using checked integer
/// arithmetic, never floating point.
fn median(env: &Env, pending: &Vec<Submission>) -> Result<i128, Error> {
    let prices = sorted_prices(env, pending);
    let n = prices.len();
    if n % 2 == 1 {
        return Ok(prices.get_unchecked(n / 2));
    }
    let mid = n / 2;
    let a = prices.get_unchecked(mid - 1);
    let b = prices.get_unchecked(mid);
    let sum = a.checked_add(b).ok_or(Error::InvalidPrice)?;
    sum.checked_div(2).ok_or(Error::InvalidPrice)
}

/// Rounds a timestamp down to the contract's resolution window, per the
/// SEP-40 design rationale: `floor(ts / resolution) * resolution`.
///
/// If the resolution is not configured yet, the timestamp is returned
/// unchanged; callers that require rounding check initialization first.
pub(crate) fn round_to_window(env: &Env, ts: u64) -> u64 {
    let resolution = env
        .storage()
        .instance()
        .get::<_, u32>(&DataKey::Resolution)
        .unwrap_or(0) as u64;
    if resolution == 0 {
        return ts;
    }
    (ts / resolution) * resolution
}

#[contractimpl]
impl Contract {
    /// Submits a price observation for a tracked commodity.
    ///
    /// ### Arguments
    /// - `node`: the submitting node. Its authorization is required, and it
    ///   must be on the authorized node list.
    /// - `asset`: the commodity the price refers to. Must be tracked.
    /// - `price`: the observed price in units of `1 / 10^decimals` of the
    ///   base asset. Must be positive.
    /// - `source_ts`: the Unix timestamp, in seconds, at which the node
    ///   observed the price.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    /// - `Err(Error::NotAuthorizedNode)` if `node` is not an authorized node.
    /// - `Err(Error::UnknownCommodity)` if `asset` is not tracked.
    /// - `Err(Error::InvalidPrice)` if `price` is not positive.
    /// - `Err(Error::DuplicateSubmission)` if `node` already has a pending
    ///   submission for `asset` in the current resolution window.
    ///
    /// ### Events
    /// Emits [`PriceSubmitted`] with the asset, node, and price.
    pub fn submit_price(
        env: Env,
        node: Address,
        asset: Asset,
        price: i128,
        source_ts: u64,
    ) -> Result<(), Error> {
        node.require_auth();
        require_initialized(&env)?;
        if !read_nodes(&env).contains(&node) {
            return Err(Error::NotAuthorizedNode);
        }
        if !read_commodities(&env).contains(&asset) {
            return Err(Error::UnknownCommodity);
        }
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }
        let now = env.ledger().timestamp();
        let window_start = round_to_window(&env, now);
        let pending = read_pending(&env, &asset);
        // A submission is in the current window when it was recorded at or
        // after the start of the current window. Pending submissions are
        // cleared on finalize, so any surviving submission from this node in
        // the current window is a duplicate.
        for sub in pending.iter() {
            if sub.node == node && sub.submitted_at >= window_start {
                return Err(Error::DuplicateSubmission);
            }
        }
        let mut pending = pending;
        pending.push_back(Submission {
            node: node.clone(),
            price,
            source_ts,
            submitted_at: now,
        });
        env.storage()
            .persistent()
            .set(&DataKey::Pending(asset.clone()), &pending);
        storage::extend_persistent(&env, &DataKey::Pending(asset.clone()));
        PriceSubmitted { asset, node, price }.publish(&env);
        Ok(())
    }

    /// Finalizes the pending submissions for a commodity into a single
    /// median price.
    ///
    /// Permissionless: any caller may trigger finalization once enough nodes
    /// have submitted. The median of all pending submission prices is written
    /// into the asset's history with the current ledger timestamp rounded to
    /// the resolution window, the history is pruned to the retention limit,
    /// and the pending submissions are cleared.
    ///
    /// ### Arguments
    /// - `asset`: the commodity to finalize.
    ///
    /// ### Returns
    /// - `Ok(PriceData)` with the finalized median price and its rounded
    ///   timestamp.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    /// - `Err(Error::NoPendingSubmissions)` if there are no pending
    ///   submissions for `asset`.
    /// - `Err(Error::ThresholdNotMet)` if the pending submission count is
    ///   below the configured threshold, or if no threshold has been
    ///   configured yet (a threshold of zero can never be met).
    /// - `Err(Error::InvalidPrice)` if the median arithmetic overflows, which
    ///   cannot occur for well-formed positive prices.
    ///
    /// ### Events
    /// Emits [`PriceFinalized`] with the asset, price, and timestamp.
    pub fn finalize_price(env: Env, asset: Asset) -> Result<PriceData, Error> {
        require_initialized(&env)?;
        let threshold = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::Threshold)
            .unwrap_or(0);
        let pending = read_pending(&env, &asset);
        if pending.is_empty() {
            return Err(Error::NoPendingSubmissions);
        }
        // A threshold of zero means no threshold was configured; it can never
        // be met, so a single node can never move a price alone.
        if threshold == 0 || pending.len() < threshold {
            return Err(Error::ThresholdNotMet);
        }
        let price = median(&env, &pending)?;
        let timestamp = round_to_window(&env, env.ledger().timestamp());
        let record = PriceData { price, timestamp };

        let mut history = read_history(&env, &asset);
        history.push_back(record.clone());
        let retention = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::Retention)
            .unwrap_or(crate::DEFAULT_RETENTION);
        // Prune the oldest records once the history exceeds the retention
        // limit so it can never grow unbounded.
        while history.len() > retention {
            history.remove(0);
        }
        env.storage()
            .persistent()
            .set(&DataKey::History(asset.clone()), &history);
        storage::extend_persistent(&env, &DataKey::History(asset.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Pending(asset.clone()));
        PriceFinalized {
            asset,
            price,
            timestamp,
        }
        .publish(&env);
        Ok(record)
    }
}
