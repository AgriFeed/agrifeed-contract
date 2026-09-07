//! Admin and node management functions.

use crate::errors::Error;
use crate::storage;
use crate::types::{Asset, DataKey};
use crate::{
    Contract, ContractArgs, ContractClient, Initialized, NodeAdded, NodeRemoved, RetentionUpdated,
    ThresholdUpdated,
};
use soroban_sdk::{contractimpl, Address, Env, Vec};

/// Returns `Error::NotInitialized` unless the contract has been initialized.
pub(crate) fn require_initialized(env: &Env) -> Result<(), Error> {
    if env.storage().instance().has(&DataKey::Admin) {
        Ok(())
    } else {
        Err(Error::NotInitialized)
    }
}

/// Reads the stored node list. Defaults to an empty list if unset, which can
/// only happen before `initialize`.
pub(crate) fn read_nodes(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Nodes)
        .unwrap_or_else(|| Vec::new(env))
}

#[contractimpl]
impl Contract {
    /// One-time setup of the oracle.
    ///
    /// ### Arguments
    /// - `admin`: the address that will manage nodes, commodities, and
    ///   configuration. Its authorization is required.
    /// - `decimals`: number of decimals used to represent prices.
    /// - `resolution`: length of one price window in seconds, used to round
    ///   all price timestamps. Must be at least 1.
    /// - `base_asset`: the base asset of this feed, per SEP-40.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::AlreadyInitialized)` if the contract was already
    ///   initialized.
    /// - `Err(Error::InvalidThreshold)` if `resolution` is zero.
    ///
    /// ### Events
    /// Emits [`Initialized`] with the admin, base asset, decimals, and
    /// resolution.
    pub fn initialize(
        env: Env,
        admin: Address,
        decimals: u32,
        resolution: u32,
        base_asset: Asset,
    ) -> Result<(), Error> {
        // Authenticate before checking state so a would-be admin cannot be
        // front-run into an unusable initialization.
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if resolution == 0 {
            return Err(Error::InvalidThreshold);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Nodes, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::Threshold, &0u32);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::Resolution, &resolution);
        env.storage().instance().set(&DataKey::BaseAsset, &base_asset);
        env.storage()
            .instance()
            .set(&DataKey::Commodities, &Vec::<Asset>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::Retention, &crate::DEFAULT_RETENTION);
        storage::extend_instance(&env);
        Initialized {
            admin,
            base_asset,
            decimals,
            resolution,
        }
        .publish(&env);
        Ok(())
    }

    /// Adds a price node to the oracle.
    ///
    /// ### Arguments
    /// - `admin`: the administrator. Its authorization is required.
    /// - `node`: the address to add as an authorized price node.
    ///
    /// ### Returns
    /// - `Ok(())` on success. Adding a node that is already present is a
    ///   no-op and still returns `Ok(())`.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    ///
    /// ### Events
    /// Emits [`NodeAdded`] with the node address when a new node is added.
    pub fn add_node(env: Env, admin: Address, node: Address) -> Result<(), Error> {
        admin.require_auth();
        require_initialized(&env)?;
        let mut nodes = read_nodes(&env);
        if nodes.contains(&node) {
            return Ok(());
        }
        nodes.push_back(node.clone());
        env.storage().instance().set(&DataKey::Nodes, &nodes);
        storage::extend_instance(&env);
        NodeAdded { node }.publish(&env);
        Ok(())
    }

    /// Removes a price node from the oracle.
    ///
    /// ### Arguments
    /// - `admin`: the administrator. Its authorization is required.
    /// - `node`: the address to remove from the authorized nodes.
    ///
    /// ### Returns
    /// - `Ok(())` on success. Removing a node that is not present is a
    ///   no-op and still returns `Ok(())`. Removal does not retroactively
    ///   invalidate that node's already-finalized prices.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    ///
    /// ### Events
    /// Emits [`NodeRemoved`] with the node address when a node is removed.
    pub fn remove_node(env: Env, admin: Address, node: Address) -> Result<(), Error> {
        admin.require_auth();
        require_initialized(&env)?;
        let mut nodes = read_nodes(&env);
        let Some(index) = nodes.first_index_of(&node) else {
            return Ok(());
        };
        nodes.remove(index);
        env.storage().instance().set(&DataKey::Nodes, &nodes);
        storage::extend_instance(&env);
        NodeRemoved { node }.publish(&env);
        Ok(())
    }

    /// Sets the minimum number of submissions required to finalize a price.
    ///
    /// ### Arguments
    /// - `admin`: the administrator. Its authorization is required.
    /// - `threshold`: the new threshold. Must be at least 1 and at most the
    ///   current number of nodes.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    /// - `Err(Error::InvalidThreshold)` if `threshold` is zero or exceeds the
    ///   number of nodes.
    ///
    /// ### Events
    /// Emits [`ThresholdUpdated`] with the new threshold.
    pub fn set_threshold(env: Env, admin: Address, threshold: u32) -> Result<(), Error> {
        admin.require_auth();
        require_initialized(&env)?;
        let nodes = read_nodes(&env);
        if threshold == 0 || threshold > nodes.len() {
            return Err(Error::InvalidThreshold);
        }
        env.storage().instance().set(&DataKey::Threshold, &threshold);
        storage::extend_instance(&env);
        ThresholdUpdated { threshold }.publish(&env);
        Ok(())
    }

    /// Sets the maximum number of finalized price records kept per commodity.
    ///
    /// Older records beyond this limit are pruned on each finalize.
    ///
    /// ### Arguments
    /// - `admin`: the administrator. Its authorization is required.
    /// - `retention`: the new retention limit. Must be at least 1.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::NotInitialized)` if the contract is not initialized.
    /// - `Err(Error::InvalidThreshold)` if `retention` is zero. The error is
    ///   reused as a general validation error for positive configuration
    ///   values.
    ///
    /// ### Events
    /// Emits [`RetentionUpdated`] with the new limit.
    pub fn set_retention(env: Env, admin: Address, retention: u32) -> Result<(), Error> {
        admin.require_auth();
        require_initialized(&env)?;
        if retention == 0 {
            return Err(Error::InvalidThreshold);
        }
        env.storage().instance().set(&DataKey::Retention, &retention);
        storage::extend_instance(&env);
        RetentionUpdated { retention }.publish(&env);
        Ok(())
    }
}