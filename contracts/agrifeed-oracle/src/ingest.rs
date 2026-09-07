//! Price ingestion: node submissions and permissionless finalization.

use crate::admin::{read_commodities, read_nodes, require_initialized};
use crate::errors::Error;
use crate::storage;
use crate::types::{Asset, DataKey, Submission};
use crate::{Contract, ContractArgs, ContractClient, PriceSubmitted};
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
        PriceSubmitted {
            asset,
            node,
            price,
        }
        .publish(&env);
        Ok(())
    }
}