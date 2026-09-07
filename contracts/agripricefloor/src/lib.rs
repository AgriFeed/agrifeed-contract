//! agripricefloor
//!
//! An example consumer of the agrifeed-oracle SEP-40 price feed. It settles
//! a cash-settled price-floor agreement between a farmer and a buyer: if the
//! market price of a commodity falls below the agreed floor at maturity, the
//! buyer pays the farmer the difference; otherwise the buyer's collateral is
//! refunded. No physical delivery and no custody of goods.
#![no_std]

mod errors;
mod types;

pub use errors::Error;
pub use types::DataKey;

use soroban_sdk::{contract, contractevent, contractimpl, Address, Env};

/// The deployed agrifeed-oracle interface.
///
/// The oracle wasm must be built (`stellar contract build --package
/// agrifeed-oracle`) before this crate compiles, since the client and the
/// shared `Asset` type are generated from its spec.
pub mod oracle {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32v1-none/release/agrifeed_oracle.wasm"
    );
}

/// Grace period, in seconds, after maturity during which an unfunded
/// agreement cannot be cancelled.
pub const UNFUNDED_CANCEL_GRACE: u64 = 48 * 60 * 60;

/// Further grace period, in seconds, that must elapse after a settle attempt
/// fails with `Error::OracleDataUnavailable` before the funded agreement can
/// be cancelled and the collateral refunded.
pub const SETTLE_FAILURE_GRACE: u64 = 48 * 60 * 60;

/// Emitted by [`Contract::initialize`] with the agreement terms.
#[contractevent]
pub struct Initialized {
    #[topic]
    pub farmer: Address,
    #[topic]
    pub buyer: Address,
    pub commodity: oracle::Asset,
    pub floor_price: i128,
    pub notional: i128,
    pub maturity_ts: u64,
}

/// Emitted by [`Contract::fund`] with the collateral amount deposited.
#[contractevent]
pub struct Funded {
    #[topic]
    pub buyer: Address,
    pub amount: i128,
}

/// Emitted by [`Contract::settle`] with the payout and the market price used.
#[contractevent]
pub struct Settled {
    #[topic]
    pub farmer: Address,
    pub payout: i128,
    pub market_price: i128,
}

/// Emitted by [`Contract::cancel`] when an agreement is cancelled.
#[contractevent]
pub struct Cancelled {
    #[topic]
    pub caller: Address,
}

#[contract]
pub struct Contract;

/// Extends the TTL of the contract instance (all agreement state) when its
/// remaining TTL drops below half of the network maximum. Called after every
/// state mutation.
fn extend_instance(env: &Env) {
    let max = env.storage().max_ttl();
    env.storage().instance().extend_ttl(max / 2, max);
}

#[contractimpl]
impl Contract {
    /// Creates a price-floor agreement between a farmer and a buyer.
    ///
    /// ### Arguments
    /// - `farmer`: the farmer protected by the floor. Its authorization is
    ///   required.
    /// - `buyer`: the buyer providing the collateral. Its authorization is
    ///   required.
    /// - `commodity`: the commodity the agreement prices, matching an asset
    ///   tracked by the oracle.
    /// - `floor_price`: the guaranteed minimum price per unit of commodity,
    ///   in the oracle's price units.
    /// - `notional`: the quantity of commodity covered by the agreement.
    /// - `settlement_token`: the token contract used for collateral and
    ///   payout.
    /// - `maturity_ts`: the Unix timestamp, in seconds, at which the
    ///   agreement matures and can be settled.
    /// - `oracle`: the SEP-40 oracle contract that prices the commodity.
    ///
    /// Both parties must sign the terms: authorization from `farmer` and
    /// `buyer` is required before anything is stored.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::AlreadyInitialized)` if the contract already holds an
    ///   agreement.
    ///
    /// ### Events
    /// Emits [`Initialized`] with the agreement terms.
    pub fn initialize(
        env: Env,
        farmer: Address,
        buyer: Address,
        commodity: oracle::Asset,
        floor_price: i128,
        notional: i128,
        settlement_token: Address,
        maturity_ts: u64,
        oracle: Address,
    ) -> Result<(), Error> {
        // Authenticate both parties before checking state so a would-be
        // front-runner cannot lock in terms the parties did not sign.
        farmer.require_auth();
        buyer.require_auth();
        if env.storage().instance().has(&DataKey::Farmer) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Farmer, &farmer.clone());
        env.storage().instance().set(&DataKey::Buyer, &buyer.clone());
        env.storage().instance().set(&DataKey::Commodity, &commodity.clone());
        env.storage().instance().set(&DataKey::FloorPrice, &floor_price);
        env.storage().instance().set(&DataKey::Notional, &notional);
        env.storage()
            .instance()
            .set(&DataKey::SettlementToken, &settlement_token.clone());
        env.storage().instance().set(&DataKey::Maturity, &maturity_ts);
        env.storage().instance().set(&DataKey::Oracle, &oracle.clone());
        env.storage().instance().set(&DataKey::Funded, &false);
        env.storage().instance().set(&DataKey::Settled, &false);
        env.storage().instance().set(&DataKey::SettleFailedAt, &0u64);
        extend_instance(&env);
        Initialized {
            farmer,
            buyer,
            commodity,
            floor_price,
            notional,
            maturity_ts,
        }
        .publish(&env);
        Ok(())
    }
}