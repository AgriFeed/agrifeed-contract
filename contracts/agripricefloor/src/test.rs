//! Unit tests for the agripricefloor contract.
#![cfg(test)]

extern crate std;

use crate::oracle;
use crate::{Contract, ContractClient, Error};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, vec, Address, Env, Symbol, Vec};

/// Test fixture: a funded, oracle-backed price-floor agreement.
struct Fixture {
    env: Env,
    pricefloor_id: Address,
    oracle_id: Address,
    nodes: Vec<Address>,
    farmer: Address,
    buyer: Address,
    stranger: Address,
    token_id: Address,
    cocoa: oracle::Asset,
    floor_price: i128,
    notional: i128,
    maturity_ts: u64,
}

fn cocoa(env: &Env) -> oracle::Asset {
    oracle::Asset::Other(Symbol::new(env, "COCOA"))
}

/// Builds an initialized agreement. The buyer is minted 1,000,000 tokens but
/// does not fund the agreement unless `fund` is called.
fn setup(floor_price: i128, notional: i128) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let farmer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let stranger = Address::generate(&env);
    let nodes = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let base_ts = 1_000_000u64;
    let maturity_ts = base_ts + 3_600;
    env.ledger().set_timestamp(base_ts);

    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    token::StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000_000);

    let commodity = cocoa(&env);
    let oracle_id = env.register(oracle::WASM, ());
    let oracle_client = oracle::Client::new(&env, &oracle_id);
    oracle_client.initialize(
        &admin,
        &7,
        &60,
        &oracle::Asset::Other(Symbol::new(&env, "USDC")),
    );
    for node in nodes.iter() {
        oracle_client.add_node(&admin, &node);
    }
    oracle_client.set_threshold(&admin, &3);
    oracle_client.add_commodity(&admin, &commodity);

    let pricefloor_id = env.register(Contract, ());
    let pf = ContractClient::new(&env, &pricefloor_id);
    pf.initialize(
        &farmer,
        &buyer,
        &commodity,
        &floor_price,
        &notional,
        &token_id,
        &maturity_ts,
        &oracle_id,
    );

    Fixture {
        env,
        pricefloor_id,
        oracle_id,
        nodes,
        farmer,
        buyer,
        stranger,
        token_id,
        cocoa: commodity,
        floor_price,
        notional,
        maturity_ts,
    }
}

/// Funds the agreement with `amount` from the buyer's minted balance.
fn fund(f: &Fixture, amount: i128) {
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    pf.fund(&f.buyer, &amount);
}

/// Makes every oracle node submit `price` and finalizes the window.
fn push_oracle_price(f: &Fixture, price: i128) {
    let oracle_client = oracle::Client::new(&f.env, &f.oracle_id);
    for node in f.nodes.iter() {
        oracle_client.submit_price(&node, &f.cocoa, &price, &f.env.ledger().timestamp());
    }
    oracle_client.finalize_price(&f.cocoa);
}

fn balance(f: &Fixture, address: &Address) -> i128 {
    token::Client::new(&f.env, &f.token_id).balance(address)
}

/// --- initialize ---

#[test]
fn test_initialize_success_and_double_init() {
    let f = setup(100, 10);
    assert_error(&f, "initialize", Error::AlreadyInitialized);
}

#[test]
fn test_initialize_requires_both_parties() {
    let env = Env::default();
    let farmer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let contract_id = env.register(Contract, ());
    let pf = ContractClient::new(&env, &contract_id);
    let commodity = cocoa(&env);
    // No auth mocking: the unauthenticated farmer makes the call fail.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pf.initialize(
            &farmer,
            &buyer,
            &commodity,
            &100,
            &10,
            &token_id,
            &1_000_000,
            &Address::generate(&env),
        );
    }));
    assert!(res.is_err());
}

/// --- fund ---

#[test]
fn test_fund_moves_tokens() {
    let f = setup(100, 10);
    fund(&f, 500);
    assert_eq!(balance(&f, &f.pricefloor_id), 500);
    assert_eq!(balance(&f, &f.buyer), 1_000_000 - 500);
}

#[test]
fn test_fund_failure_paths() {
    let f = setup(100, 10);
    fund(&f, 500);
    // Second funding attempt.
    assert_error(&f, "fund_twice", Error::AlreadyFunded);
    // Zero and negative amounts are rejected.
    assert_error(&f, "fund_zero", Error::InvalidAmount);
    // A stranger cannot fund.
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let res = pf.try_fund(&f.stranger, &100);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

/// --- settle ---

#[test]
fn test_settle_before_maturity_fails() {
    let f = setup(100, 10);
    fund(&f, 500);
    // Still at base_ts, well before maturity.
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let res = pf.try_settle();
    assert_eq!(res, Err(Ok(Error::NotYetMature)));
}

#[test]
fn test_settle_without_funding_fails() {
    let f = setup(100, 10);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let res = pf.try_settle();
    assert_eq!(res, Err(Ok(Error::NotFunded)));
}

#[test]
fn test_settle_pays_farmer_below_floor() {
    let f = setup(100, 10);
    fund(&f, 500);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    push_oracle_price(&f, 80);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let payout = pf.settle();
    // (100 - 80) * 10 = 200 below the 500 collateral cap.
    assert_eq!(payout, 200);
    assert_eq!(balance(&f, &f.farmer), 200);
    assert_eq!(balance(&f, &f.buyer), 1_000_000 - 500 + 300);
    assert_eq!(balance(&f, &f.pricefloor_id), 0);
}

#[test]
fn test_settle_caps_payout_at_collateral() {
    let f = setup(100, 10);
    fund(&f, 150);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    push_oracle_price(&f, 80);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    // (100 - 80) * 10 = 200 exceeds the 150 collateral, so payout is capped.
    let payout = pf.settle();
    assert_eq!(payout, 150);
    assert_eq!(balance(&f, &f.farmer), 150);
    assert_eq!(balance(&f, &f.buyer), 1_000_000 - 150);
    assert_eq!(balance(&f, &f.pricefloor_id), 0);
}

#[test]
fn test_settle_refunds_at_or_above_floor() {
    let f = setup(100, 10);
    fund(&f, 500);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    push_oracle_price(&f, 150);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let payout = pf.settle();
    assert_eq!(payout, 0);
    assert_eq!(balance(&f, &f.farmer), 0);
    assert_eq!(balance(&f, &f.buyer), 1_000_000);
    assert_eq!(balance(&f, &f.pricefloor_id), 0);
}

#[test]
fn test_settle_once_only() {
    let f = setup(100, 10);
    fund(&f, 500);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    push_oracle_price(&f, 80);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    pf.settle();
    let res = pf.try_settle();
    assert_eq!(res, Err(Ok(Error::AlreadySettled)));
}

#[test]
fn test_settle_without_oracle_price() {
    let f = setup(100, 10);
    fund(&f, 500);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    // No price was ever finalized for COCOA, so settlement is impossible.
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let res = pf.try_settle();
    assert_eq!(res, Err(Ok(Error::OracleDataUnavailable)));
    // The funded agreement can be cancelled once the further grace window
    // past maturity has elapsed, refunding the collateral to the buyer.
    f.env
        .ledger()
        .set_timestamp(f.maturity_ts + crate::SETTLE_FAILURE_GRACE + 1);
    let res = pf.try_cancel(&f.buyer);
    assert_eq!(res, Ok(Ok(())));
    assert_eq!(balance(&f, &f.buyer), 1_000_000);
}

#[test]
fn test_cancel_funded_refuses_when_oracle_has_price() {
    let f = setup(100, 10);
    fund(&f, 500);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    push_oracle_price(&f, 80);
    // Even well past the grace window, a funded agreement with a priced
    // commodity must settle rather than cancel.
    f.env
        .ledger()
        .set_timestamp(f.maturity_ts + crate::SETTLE_FAILURE_GRACE * 2);
    let res = pf.try_cancel(&f.buyer);
    assert_eq!(res, Err(Ok(Error::GracePeriodNotElapsed)));
}

/// --- cancel ---

#[test]
fn test_cancel_requires_party() {
    let f = setup(100, 10);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let res = pf.try_cancel(&f.stranger);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_cancel_unfunded_before_grace_fails() {
    let f = setup(100, 10);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    // After maturity but inside the 48-hour grace window.
    f.env.ledger().set_timestamp(f.maturity_ts + 3_600);
    let res = pf.try_cancel(&f.farmer);
    assert_eq!(res, Err(Ok(Error::GracePeriodNotElapsed)));
}

#[test]
fn test_cancel_unfunded_after_grace_succeeds() {
    let f = setup(100, 10);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    f.env
        .ledger()
        .set_timestamp(f.maturity_ts + crate::UNFUNDED_CANCEL_GRACE + 1);
    let res = pf.try_cancel(&f.farmer);
    assert_eq!(res, Ok(Ok(())));
}

#[test]
fn test_cancel_funded_requires_grace_elapsed() {
    let f = setup(100, 10);
    fund(&f, 500);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    // Maturity plus the grace window has not elapsed yet and the oracle is
    // silent, so cancel is premature.
    f.env
        .ledger()
        .set_timestamp(f.maturity_ts + crate::SETTLE_FAILURE_GRACE - 1);
    let res = pf.try_cancel(&f.buyer);
    assert_eq!(res, Err(Ok(Error::GracePeriodNotElapsed)));
    // Once the grace window has elapsed with the oracle still silent, the
    // buyer can cancel and recover the collateral.
    f.env
        .ledger()
        .set_timestamp(f.maturity_ts + crate::SETTLE_FAILURE_GRACE + 1);
    let res = pf.try_cancel(&f.buyer);
    assert_eq!(res, Ok(Ok(())));
    assert_eq!(balance(&f, &f.pricefloor_id), 0);
    assert_eq!(balance(&f, &f.buyer), 1_000_000);
}

#[test]
fn test_cancel_after_settle_fails() {
    let f = setup(100, 10);
    fund(&f, 500);
    f.env.ledger().set_timestamp(f.maturity_ts + 1);
    push_oracle_price(&f, 80);
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    pf.settle();
    let res = pf.try_cancel(&f.farmer);
    assert_eq!(res, Err(Ok(Error::AlreadySettled)));
}

/// Asserts that calling a pricefloor function returns the given typed error.
fn assert_error(f: &Fixture, case: &str, expected: Error) {
    let pf = ContractClient::new(&f.env, &f.pricefloor_id);
    let res = match case {
        "initialize" => pf.try_initialize(
            &f.farmer,
            &f.buyer,
            &f.cocoa,
            &f.floor_price,
            &f.notional,
            &f.token_id,
            &f.maturity_ts,
            &f.oracle_id,
        ),
        "fund_twice" => pf.try_fund(&f.buyer, &500),
        "fund_zero" => pf.try_fund(&f.buyer, &0),
        _ => unreachable!("unknown case {case}"),
    };
    assert_eq!(res, Err(Ok(expected)));
}
