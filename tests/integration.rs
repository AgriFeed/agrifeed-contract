//! End-to-end integration test across the agrifeed contracts.
//! Contract interface signatures are fixed by the application spec, so the
//! generated clients trip clippy::too_many_arguments.
#![allow(clippy::too_many_arguments)]
//!
//! Deploys the oracle from its wasm, ingests three node prices for COCOA,
//! finalizes the median, then runs a funded price-floor agreement to
//! maturity and settles it against the oracle price. Both wasms must be
//! built first (`stellar contract build --package agrifeed-oracle` and
//! `stellar contract build --package agripricefloor`).

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, vec, Address, Env, Symbol};

mod oracle {
    soroban_sdk::contractimport!(file = "target/wasm32v1-none/release/agrifeed_oracle.wasm");
}

mod pricefloor {
    // The pricefloor spec reuses the oracle's Asset type without exporting it.
    pub use crate::oracle::Asset;
    soroban_sdk::contractimport!(file = "target/wasm32v1-none/release/agripricefloor.wasm");
}

fn cocoa(env: &Env) -> oracle::Asset {
    oracle::Asset::Other(Symbol::new(env, "COCOA"))
}

#[test]
fn full_flow_oracle_to_settlement() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let farmer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let nodes = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];

    // One settlement token shared by the flow.
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_id);
    token::StellarAssetClient::new(&env, &token_id).mint(&buyer, &10_000);

    // --- deploy and configure the oracle ---
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
    oracle_client.add_commodity(&admin, &cocoa(&env));
    assert_eq!(oracle_client.assets(), vec![&env, cocoa(&env)]);

    // --- three nodes submit different prices, then finalize ---
    let base_ts = 5_000_000u64;
    let maturity_ts = base_ts + 3_600;
    env.ledger().set_timestamp(base_ts);
    oracle_client.submit_price(&nodes.get_unchecked(0), &cocoa(&env), &90, &(base_ts - 2));
    oracle_client.submit_price(&nodes.get_unchecked(1), &cocoa(&env), &110, &(base_ts - 1));
    oracle_client.submit_price(&nodes.get_unchecked(2), &cocoa(&env), &100, &base_ts);
    let finalized = oracle_client.finalize_price(&cocoa(&env));
    assert_eq!(finalized.price, 100);
    assert_eq!(
        oracle_client.lastprice(&cocoa(&env)),
        Some(finalized.clone())
    );
    assert_eq!(
        oracle_client.prices(&cocoa(&env), &1),
        Some(vec![&env, finalized])
    );

    // --- deploy the price-floor agreement against the oracle ---
    let pricefloor_id = env.register(pricefloor::WASM, ());
    let pf = pricefloor::Client::new(&env, &pricefloor_id);
    // The finalized median price is 100. A floor of 120 and notional of 10
    // settle a payout of (120 - 100) * 10 = 200 to the farmer, refunding the
    // remaining 800 of the 1,000 collateral to the buyer.
    pf.initialize(
        &farmer,
        &buyer,
        &cocoa(&env),
        &120,
        &10,
        &token_id,
        &maturity_ts,
        &oracle_id,
    );
    pf.fund(&buyer, &1_000);
    assert_eq!(token_client.balance(&pricefloor_id), 1_000);
    assert_eq!(token_client.balance(&buyer), 9_000);

    // --- settle after maturity ---
    env.ledger().set_timestamp(maturity_ts + 60);
    let payout = pf.settle();
    assert_eq!(payout, 200);
    assert_eq!(token_client.balance(&farmer), 200);
    assert_eq!(token_client.balance(&buyer), 9_800);
    assert_eq!(token_client.balance(&pricefloor_id), 0);
}
