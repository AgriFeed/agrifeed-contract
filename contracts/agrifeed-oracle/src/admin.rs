//! Admin and node management functions.

use crate::errors::Error;
use crate::storage;
use crate::types::{Asset, DataKey};
use crate::{Contract, ContractArgs, ContractClient, Initialized};
use soroban_sdk::{contractimpl, Address, Env, Vec};

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
}