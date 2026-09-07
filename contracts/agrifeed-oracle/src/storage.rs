//! TTL housekeeping helpers.
//!
//! Soroban storage entries expire unless their TTL is extended. Every write
//! in this contract extends the TTL of the affected entries in the same call,
//! and the permissionless `extend_instance_ttl` function lets any relayer
//! keep idle configuration alive.

use crate::types::DataKey;
use soroban_sdk::Env;

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