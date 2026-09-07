//! agripricefloor
//!
//! An example consumer of the agrifeed-oracle SEP-40 price feed. It settles
//! a cash-settled price-floor agreement between a farmer and a buyer: if the
//! market price of a commodity falls below the agreed floor at maturity, the
//! buyer pays the farmer the difference; otherwise the buyer's collateral is
//! refunded. No physical delivery and no custody of goods.
#![no_std]
// Public function signatures are fixed by the application spec (for example
// initialize takes nine arguments), so the lint is allowed at crate level.
#![allow(clippy::too_many_arguments)]

mod errors;
#[cfg(test)]
mod test;
mod types;

pub use errors::Error;
pub use types::DataKey;

use soroban_sdk::{contract, contractevent, contractimpl, token, Address, Env};

/// The deployed agrifeed-oracle interface.
///
/// The oracle wasm must be built (`stellar contract build --package
/// agrifeed-oracle`) before this crate compiles, since the client and the
/// shared `Asset` type are generated from its spec.
pub mod oracle {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/agrifeed_oracle.wasm");
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
        env.storage()
            .instance()
            .set(&DataKey::Farmer, &farmer.clone());
        env.storage()
            .instance()
            .set(&DataKey::Buyer, &buyer.clone());
        env.storage()
            .instance()
            .set(&DataKey::Commodity, &commodity.clone());
        env.storage()
            .instance()
            .set(&DataKey::FloorPrice, &floor_price);
        env.storage().instance().set(&DataKey::Notional, &notional);
        env.storage()
            .instance()
            .set(&DataKey::SettlementToken, &settlement_token.clone());
        env.storage()
            .instance()
            .set(&DataKey::Maturity, &maturity_ts);
        env.storage()
            .instance()
            .set(&DataKey::Oracle, &oracle.clone());
        env.storage().instance().set(&DataKey::Funded, &false);
        env.storage().instance().set(&DataKey::Settled, &false);
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

    /// Deposits the buyer's collateral into the contract.
    ///
    /// ### Arguments
    /// - `buyer`: the address funding the agreement. Its authorization is
    ///   required and it must be the buyer recorded at `initialize`.
    /// - `amount`: the amount of `settlement_token` to transfer in. Must be
    ///   positive.
    ///
    /// ### Returns
    /// - `Ok(())` on success, after the tokens are in the contract.
    /// - `Err(Error::Unauthorized)` if `buyer` is not the stored buyer, which
    ///   is also the case when no agreement exists yet.
    /// - `Err(Error::AlreadyFunded)` if the agreement is already funded.
    /// - `Err(Error::AlreadySettled)` if the agreement already settled.
    /// - `Err(Error::InvalidAmount)` if `amount` is not positive.
    ///
    /// ### Events
    /// Emits [`Funded`] with the deposited amount.
    pub fn fund(env: Env, buyer: Address, amount: i128) -> Result<(), Error> {
        buyer.require_auth();
        // The buyer identity check doubles as the initialization guard: when
        // no agreement exists there is no stored buyer to match.
        let stored_buyer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Buyer)
            .ok_or(Error::Unauthorized)?;
        if stored_buyer != buyer {
            return Err(Error::Unauthorized);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let funded: bool = env
            .storage()
            .instance()
            .get(&DataKey::Funded)
            .unwrap_or(false);
        if funded {
            return Err(Error::AlreadyFunded);
        }
        let settled: bool = env
            .storage()
            .instance()
            .get(&DataKey::Settled)
            .unwrap_or(false);
        if settled {
            return Err(Error::AlreadySettled);
        }
        let settlement_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::SettlementToken)
            .ok_or(Error::Unauthorized)?;
        let token_client = token::Client::new(&env, &settlement_token);
        token_client.transfer(&buyer, env.current_contract_address(), &amount);
        env.storage().instance().set(&DataKey::Funded, &true);
        extend_instance(&env);
        Funded { buyer, amount }.publish(&env);
        Ok(())
    }

    /// Settles the agreement at maturity against the oracle's market price.
    ///
    /// Permissionless: anyone may trigger settlement once the maturity
    /// timestamp has passed and the collateral is in the contract. The oracle
    /// is queried with `lastprice`; if it has no price the contract never
    /// fabricates a payout and returns
    /// `Err(Error::OracleDataUnavailable)`. Soroban rolls back all state
    /// changes of a failed invocation, so `cancel` infers the stalled state
    /// from the oracle directly rather than from a recorded failure.
    ///
    /// When the market price is below the floor, the farmer receives
    /// `min((floor_price - market_price) * notional, funded_amount)` where
    /// `funded_amount` is the contract's settlement token balance, and the
    /// buyer receives the remainder. When the market price is at or above the
    /// floor, the buyer receives the full collateral back.
    ///
    /// ### Returns
    /// - `Ok(i128)` with the payout sent to the farmer (zero when the market
    ///   was at or above the floor).
    /// - `Err(Error::AlreadySettled)` if the agreement already settled.
    /// - `Err(Error::NotYetMature)` if called before the maturity timestamp.
    /// - `Err(Error::NotFunded)` if the collateral was never deposited, which
    ///   is also the case when no agreement exists.
    /// - `Err(Error::OracleDataUnavailable)` if the oracle has no price for
    ///   the commodity.
    /// - `Err(Error::ArithmeticOverflow)` if the payout math overflows, which
    ///   cannot occur for well-formed prices.
    ///
    /// ### Events
    /// Emits [`Settled`] with the payout and the market price used.
    pub fn settle(env: Env) -> Result<i128, Error> {
        let now = env.ledger().timestamp();
        let settled: bool = env
            .storage()
            .instance()
            .get(&DataKey::Settled)
            .unwrap_or(false);
        if settled {
            return Err(Error::AlreadySettled);
        }
        let maturity: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Maturity)
            .ok_or(Error::NotYetMature)?;
        if now < maturity {
            return Err(Error::NotYetMature);
        }
        let funded: bool = env
            .storage()
            .instance()
            .get(&DataKey::Funded)
            .unwrap_or(false);
        if !funded {
            return Err(Error::NotFunded);
        }

        let farmer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Farmer)
            .ok_or(Error::NotFunded)?;
        let buyer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Buyer)
            .ok_or(Error::NotFunded)?;
        let commodity: oracle::Asset = env
            .storage()
            .instance()
            .get(&DataKey::Commodity)
            .ok_or(Error::NotFunded)?;
        let floor_price: i128 = env
            .storage()
            .instance()
            .get(&DataKey::FloorPrice)
            .ok_or(Error::NotFunded)?;
        let notional: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Notional)
            .ok_or(Error::NotFunded)?;
        let settlement_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::SettlementToken)
            .ok_or(Error::NotFunded)?;
        let oracle_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::NotFunded)?;
        // The reads above are only None on a contract without a funded
        // agreement; NotFunded is returned for that case rather than panic.

        let token_client = token::Client::new(&env, &settlement_token);
        let oracle_client = oracle::Client::new(&env, &oracle_addr);
        let market_price = match oracle_client.lastprice(&commodity) {
            Some(price_data) => price_data.price,
            // Never invent a settlement price. Note that Soroban rolls back all
            // state changes of a failed invocation, so a failure marker cannot
            // be persisted here; cancel infers the stalled state from the
            // oracle directly instead.
            None => return Err(Error::OracleDataUnavailable),
        };

        let funded_amount = token_client.balance(&env.current_contract_address());
        let (payout, refund) = if market_price < floor_price {
            let diff = floor_price
                .checked_sub(market_price)
                .ok_or(Error::ArithmeticOverflow)?;
            let gross = diff
                .checked_mul(notional)
                .ok_or(Error::ArithmeticOverflow)?;
            let payout = core::cmp::min(gross, funded_amount);
            let refund = funded_amount
                .checked_sub(payout)
                .ok_or(Error::ArithmeticOverflow)?;
            (payout, refund)
        } else {
            (0, funded_amount)
        };

        if payout > 0 {
            token_client.transfer(&env.current_contract_address(), &farmer, &payout);
        }
        if refund > 0 {
            token_client.transfer(&env.current_contract_address(), &buyer, &refund);
        }
        env.storage().instance().set(&DataKey::Settled, &true);
        extend_instance(&env);
        Settled {
            farmer,
            payout,
            market_price,
        }
        .publish(&env);
        Ok(payout)
    }

    /// Cancels the agreement and refunds any collateral.
    ///
    /// ### Arguments
    /// - `caller`: the address requesting the cancel. Its authorization is
    ///   required, and it must be the farmer or the buyer.
    ///
    /// Cancellation is available only when the agreement has stalled:
    /// - while unfunded, once the maturity timestamp plus the unfunded grace
    ///   period ([`UNFUNDED_CANCEL_GRACE`]) has passed, or
    /// - while funded, once the maturity timestamp plus the further grace
    ///   period ([`SETTLE_FAILURE_GRACE`]) has passed and the oracle still
    ///   has no price for the commodity, meaning `settle` keeps failing with
    ///   [`Error::OracleDataUnavailable`] and the collateral would be locked
    ///   forever. Soroban rolls back a failed invocation, so a settle attempt
    ///   cannot persist a failure marker; the oracle state is checked
    ///   directly instead. If the oracle has a price, the agreement can be
    ///   settled normally and cancel is refused. In the refund case the full
    ///   collateral balance goes back to the buyer.
    ///
    /// An unfunded cancel changes no state, since nothing was deposited.
    ///
    /// ### Returns
    /// - `Ok(())` on success.
    /// - `Err(Error::Unauthorized)` if `caller` is neither party, which is
    ///   also the case when no agreement exists.
    /// - `Err(Error::AlreadySettled)` if the agreement already settled.
    /// - `Err(Error::GracePeriodNotElapsed)` if the applicable grace period
    ///   has not elapsed yet, including when the oracle has a price and the
    ///   agreement can still be settled.
    ///
    /// ### Events
    /// Emits [`Cancelled`] with the caller.
    pub fn cancel(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let farmer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Farmer)
            .ok_or(Error::Unauthorized)?;
        let buyer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Buyer)
            .ok_or(Error::Unauthorized)?;
        if caller != farmer && caller != buyer {
            return Err(Error::Unauthorized);
        }
        let settled: bool = env
            .storage()
            .instance()
            .get(&DataKey::Settled)
            .unwrap_or(false);
        if settled {
            return Err(Error::AlreadySettled);
        }
        let now = env.ledger().timestamp();
        let funded: bool = env
            .storage()
            .instance()
            .get(&DataKey::Funded)
            .unwrap_or(false);
        let settlement_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::SettlementToken)
            .ok_or(Error::Unauthorized)?;
        let token_client = token::Client::new(&env, &settlement_token);
        if funded {
            let maturity: u64 = env
                .storage()
                .instance()
                .get(&DataKey::Maturity)
                .ok_or(Error::GracePeriodNotElapsed)?;
            // A funded agreement can only be cancelled once the further grace
            // window past maturity has elapsed and the oracle still has no
            // price, so settlement keeps failing. When the oracle has a price
            // the agreement should be settled instead of cancelled.
            if now < maturity.saturating_add(SETTLE_FAILURE_GRACE) {
                return Err(Error::GracePeriodNotElapsed);
            }
            let oracle_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Oracle)
                .ok_or(Error::GracePeriodNotElapsed)?;
            let commodity: oracle::Asset = env
                .storage()
                .instance()
                .get(&DataKey::Commodity)
                .ok_or(Error::GracePeriodNotElapsed)?;
            let oracle_client = oracle::Client::new(&env, &oracle_addr);
            if oracle_client.lastprice(&commodity).is_some() {
                return Err(Error::GracePeriodNotElapsed);
            }
            let balance = token_client.balance(&env.current_contract_address());
            if balance > 0 {
                token_client.transfer(&env.current_contract_address(), &buyer, &balance);
            }
            env.storage().instance().set(&DataKey::Funded, &false);
        } else {
            let maturity: u64 = env
                .storage()
                .instance()
                .get(&DataKey::Maturity)
                .ok_or(Error::GracePeriodNotElapsed)?;
            if now < maturity.saturating_add(UNFUNDED_CANCEL_GRACE) {
                return Err(Error::GracePeriodNotElapsed);
            }
            // Nothing was deposited, so there is nothing to refund.
        }
        extend_instance(&env);
        Cancelled { caller }.publish(&env);
        Ok(())
    }
}
