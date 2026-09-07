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

use soroban_sdk::{contract, contractevent, Address};

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