//! TTL housekeeping helpers.
//!
//! Soroban storage entries expire unless their TTL is extended. Every write
//! in this contract extends the TTL of the affected entries in the same call,
//! and the permissionless `extend_instance_ttl` function lets any relayer
//! keep idle configuration alive.

use crate::admin::require_initialized;
use crate::errors::Error;
use crate::types::{Asset, DataKey};
use crate::{Contract, ContractArgs, ContractClient};
use soroban_sdk::{contractimpl, Env};

/// Extends the TTL of the contract's instance storage when its remaining TTL
/// drops below half of the network maximum.
///
/// Instance storage is a single ledger entry, so extending any instance key
/// extends the whole instance.
pub(crate) fn extend_instance(env: &Env) {
    let max = env.storage().max_ttl();
    env.storage().instance().extend_ttl(max / 2, max);
}

/// Extends the TTL of a persistent storage entry when its remaining TTL
/// drops below half of the network maximum.
pub(crate) fn extend_persistent(env: &Env, key: &DataKey) {
    let max = env.storage().max_ttl();
    env.storage().persistent().extend_ttl(key, max / 2, max);
}

#[contractimpl]
impl Contract {
    /// Extends the TTL of the oracle's storage so it does not get archived
    /// from inactivity.
    ///
    /// Permissionless: anyone may call it. The instance storage (admin
    /// configuration) is always extended; the pending submissions and
    /// finalized history for `asset` are extended when they exist. Relayers
    /// should call this periodically, for example from a cron job, for every
    /// tracked commodity.
    ///
    /// ### Arguments
    /// - `asset`: the commodity whose persistent entries should be kept
    ///   alive.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    pub fn extend_instance_ttl(env: Env, asset: Asset) -> Result<(), Error> {
        require_initialized(&env)?;
        extend_instance(&env);
        if env.storage().persistent().has(&DataKey::Pending(asset.clone())) {
            extend_persistent(&env, &DataKey::Pending(asset.clone()));
        }
        if env.storage().persistent().has(&DataKey::History(asset.clone())) {
            extend_persistent(&env, &DataKey::History(asset));
        }
        Ok(())
    }
}