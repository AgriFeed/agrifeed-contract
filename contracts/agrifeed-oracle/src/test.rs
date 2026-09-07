//! Unit tests for the agrifeed-oracle contract.
#![cfg(test)]

extern crate std;

use crate::{
    Asset, Contract, ContractClient, DataKey, Error, PriceData, PriceFinalized, Submission,
};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{vec, Address, Env, Event, IntoVal, Symbol, Val, Vec};

/// Test fixture: an initialized oracle with three nodes and one commodity.
///
/// Auth is mocked for the whole env, so every client call authorizes as the
/// caller it names.
struct Fixture {
    env: Env,
    contract_id: Address,
    admin: Address,
    nodes: Vec<Address>,
    cocoa: Asset,
}

fn asset(env: &Env, name: &str) -> Asset {
    Asset::Other(Symbol::new(env, name))
}

fn setup() -> Fixture {
    let env = Env::default();
    let admin = Address::generate(&env);
    let nodes = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let cocoa = asset(&env, "COCOA");
    let contract_id = env.register(Contract, ());
    // Permanent env-level auth mocking: every invocation on this env is
    // authorized, including raw try_invoke_contract calls. Client-level
    // mocks in individual tests are then redundant but harmless.
    env.mock_all_auths();
    let client = ContractClient::new(&env, &contract_id);
    client
        .mock_all_auths()
        .initialize(&admin, &7, &60, &asset(&env, "USDC"));
    for node in nodes.iter() {
        client.mock_all_auths().add_node(&admin, &node);
    }
    client.mock_all_auths().set_threshold(&admin, &3);
    client.mock_all_auths().add_commodity(&admin, &cocoa);
    Fixture {
        env,
        contract_id,
        admin,
        nodes,
        cocoa,
    }
}

/// Submits one price from every node in the fixture.
fn submit_all(f: &Fixture, price: i128) {
    let client = ContractClient::new(&f.env, &f.contract_id);
    for node in f.nodes.iter() {
        client
            .mock_all_auths()
            .submit_price(&node, &f.cocoa, &price, &(price as u64));
    }
}

/// Asserts that calling `func` with `args` fails with the given typed error.
///
/// Uses `try_invoke_contract` because the generated client unwraps the
/// contract Result and panics on errors instead of returning them. A typed
/// contract error surfaces as `Err(Ok(error))`; `Err(Err(_))` would mean a
/// host-level failure such as missing auth.
fn assert_error(env: &Env, contract_id: &Address, func: &str, args: Vec<Val>, expected: Error) {
    let res = env.try_invoke_contract::<Result<(), Error>, Error>(
        contract_id,
        &Symbol::new(env, func),
        args,
    );
    assert_eq!(res, Err(Ok(expected)));
}

/// --- initialize ---

#[test]
fn test_initialize_sets_config() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    assert_eq!(client.decimals(), 7);
    assert_eq!(client.resolution(), 60);
    assert_eq!(client.base(), asset(&f.env, "USDC"));
}

#[test]
fn test_initialize_twice_fails() {
    let f = setup();
    assert_error(
        &f.env,
        &f.contract_id,
        "initialize",
        vec![
            &f.env,
            f.admin.clone().into_val(&f.env),
            7u32.into_val(&f.env),
            60u32.into_val(&f.env),
            asset(&f.env, "USDC").into_val(&f.env),
        ],
        Error::AlreadyInitialized,
    );
}

#[test]
fn test_initialize_zero_resolution_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    env.mock_all_auths();
    assert_error(
        &env,
        &contract_id,
        "initialize",
        vec![
            &env,
            admin.clone().into_val(&env),
            7u32.into_val(&env),
            0u32.into_val(&env),
            asset(&env, "USDC").into_val(&env),
        ],
        Error::InvalidThreshold,
    );
}

#[test]
fn test_initialize_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    // No auth mocking: the caller cannot authorize as admin, so the
    // invocation fails with a host auth error before any state is written.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.initialize(&admin, &7, &60, &asset(&env, "USDC"));
    }));
    assert!(res.is_err());
}

/// --- node management ---

#[test]
fn test_add_and_remove_node() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    let extra = Address::generate(&f.env);
    client.mock_all_auths().add_node(&f.admin, &extra);
    // Removing the extra node again succeeds.
    client.mock_all_auths().remove_node(&f.admin, &extra);
    // Removing an unknown node is a no-op.
    client.mock_all_auths().remove_node(&f.admin, &extra);
}

#[test]
fn test_add_node_twice_is_noop() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    let extra = Address::generate(&f.env);
    client.mock_all_auths().add_node(&f.admin, &extra);
    client.mock_all_auths().add_node(&f.admin, &extra);
}

#[test]
fn test_admin_functions_require_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let node = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    env.mock_all_auths();
    assert_error(
        &env,
        &contract_id,
        "add_node",
        vec![&env, admin.clone().into_val(&env), node.clone().into_val(&env)],
        Error::NotInitialized,
    );
    assert_error(
        &env,
        &contract_id,
        "remove_node",
        vec![&env, admin.clone().into_val(&env), node.clone().into_val(&env)],
        Error::NotInitialized,
    );
    assert_error(
        &env,
        &contract_id,
        "set_threshold",
        vec![&env, admin.clone().into_val(&env), 1u32.into_val(&env)],
        Error::NotInitialized,
    );
    assert_error(
        &env,
        &contract_id,
        "set_retention",
        vec![&env, admin.clone().into_val(&env), 90u32.into_val(&env)],
        Error::NotInitialized,
    );
    assert_error(
        &env,
        &contract_id,
        "add_commodity",
        vec![
            &env,
            admin.clone().into_val(&env),
            asset(&env, "COCOA").into_val(&env),
        ],
        Error::NotInitialized,
    );
}

#[test]
fn test_admin_functions_require_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    // Initialize with mocked auth so the contract is in a usable state.
    client
        .mock_all_auths()
        .initialize(&admin, &7, &60, &asset(&env, "USDC"));
    let stranger = Address::generate(&env);
    let node = Address::generate(&env);
    // No further mocking for the calls under test.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.add_node(&stranger, &node);
    }));
    assert!(res.is_err());
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_threshold(&stranger, &2);
    }));
    assert!(res.is_err());
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.add_commodity(&stranger, &asset(&env, "COFFEE"));
    }));
    assert!(res.is_err());
}

/// --- threshold and retention ---

#[test]
fn test_set_threshold_validates() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    assert_error(
        &f.env,
        &f.contract_id,
        "set_threshold",
        vec![&f.env, f.admin.clone().into_val(&f.env), 4u32.into_val(&f.env)],
        Error::InvalidThreshold,
    );
    assert_error(
        &f.env,
        &f.contract_id,
        "set_threshold",
        vec![&f.env, f.admin.clone().into_val(&f.env), 0u32.into_val(&f.env)],
        Error::InvalidThreshold,
    );
    client.mock_all_auths().set_threshold(&f.admin, &2);
}

#[test]
fn test_set_retention_validates() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    assert_error(
        &f.env,
        &f.contract_id,
        "set_retention",
        vec![&f.env, f.admin.clone().into_val(&f.env), 0u32.into_val(&f.env)],
        Error::InvalidThreshold,
    );
    client.mock_all_auths().set_retention(&f.admin, &5);
}

/// --- commodities ---

#[test]
fn test_add_commodity_duplicate_fails() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    assert_error(
        &f.env,
        &f.contract_id,
        "add_commodity",
        vec![
            &f.env,
            f.admin.clone().into_val(&f.env),
            f.cocoa.clone().into_val(&f.env),
        ],
        Error::CommodityAlreadyExists,
    );
    client
        .mock_all_auths()
        .add_commodity(&f.admin, &asset(&f.env, "COFFEE"));
    assert_eq!(client.assets().len(), 2);
}

/// --- submit_price ---

#[test]
fn test_submit_price_success_and_failure_paths() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    let stranger = Address::generate(&f.env);
    let coffee = asset(&f.env, "COFFEE");

    assert_error(
        &f.env,
        &f.contract_id,
        "submit_price",
        vec![
            &f.env,
            stranger.clone().into_val(&f.env),
            f.cocoa.clone().into_val(&f.env),
            100i128.into_val(&f.env),
            100u64.into_val(&f.env),
        ],
        Error::NotAuthorizedNode,
    );
    assert_error(
        &f.env,
        &f.contract_id,
        "submit_price",
        vec![
            &f.env,
            f.nodes.get_unchecked(0).clone().into_val(&f.env),
            coffee.clone().into_val(&f.env),
            100i128.into_val(&f.env),
            100u64.into_val(&f.env),
        ],
        Error::UnknownCommodity,
    );
    assert_error(
        &f.env,
        &f.contract_id,
        "submit_price",
        vec![
            &f.env,
            f.nodes.get_unchecked(0).clone().into_val(&f.env),
            f.cocoa.clone().into_val(&f.env),
            0i128.into_val(&f.env),
            100u64.into_val(&f.env),
        ],
        Error::InvalidPrice,
    );
    assert_error(
        &f.env,
        &f.contract_id,
        "submit_price",
        vec![
            &f.env,
            f.nodes.get_unchecked(0).clone().into_val(&f.env),
            f.cocoa.clone().into_val(&f.env),
            (-5i128).into_val(&f.env),
            100u64.into_val(&f.env),
        ],
        Error::InvalidPrice,
    );
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &100, &100);
}

#[test]
fn test_submit_price_requires_node_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    client
        .mock_all_auths()
        .initialize(&admin, &7, &60, &asset(&env, "USDC"));
    let stranger = Address::generate(&env);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.submit_price(&stranger, &asset(&env, "COCOA"), &100, &100);
    }));
    assert!(res.is_err());
}

#[test]
fn test_submit_price_duplicate_within_window() {
    let f = setup();
    f.env.ledger().set_timestamp(1_000);
    let client = ContractClient::new(&f.env, &f.contract_id);
    let node = f.nodes.get_unchecked(0).clone();
    client
        .mock_all_auths()
        .submit_price(&node, &f.cocoa, &100, &100);
    assert_error(
        &f.env,
        &f.contract_id,
        "submit_price",
        vec![
            &f.env,
            node.clone().into_val(&f.env),
            f.cocoa.clone().into_val(&f.env),
            200i128.into_val(&f.env),
            200u64.into_val(&f.env),
        ],
        Error::DuplicateSubmission,
    );
    // After the window rolls over the same node may submit again.
    f.env.ledger().set_timestamp(1_100);
    client
        .mock_all_auths()
        .submit_price(&node, &f.cocoa, &300, &300);
}

/// --- finalize_price ---

#[test]
fn test_finalize_price_below_threshold() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &100, &100);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(1), &f.cocoa, &200, &200);
    let res = f.env.try_invoke_contract::<Result<PriceData, Error>, Error>(
        &f.contract_id,
        &Symbol::new(&f.env, "finalize_price"),
        vec![&f.env, f.cocoa.clone().into_val(&f.env)],
    );
    assert_eq!(res, Err(Ok(Error::ThresholdNotMet)));
}

#[test]
fn test_finalize_price_no_pending() {
    let f = setup();
    let res = f.env.try_invoke_contract::<Result<PriceData, Error>, Error>(
        &f.contract_id,
        &Symbol::new(&f.env, "finalize_price"),
        vec![&f.env, f.cocoa.clone().into_val(&f.env)],
    );
    assert_eq!(res, Err(Ok(Error::NoPendingSubmissions)));
    let res = f.env.try_invoke_contract::<Result<PriceData, Error>, Error>(
        &f.contract_id,
        &Symbol::new(&f.env, "finalize_price"),
        vec![&f.env, asset(&f.env, "COFFEE").into_val(&f.env)],
    );
    assert_eq!(res, Err(Ok(Error::NoPendingSubmissions)));
}

#[test]
fn test_finalize_price_odd_median() {
    let f = setup();
    f.env.ledger().set_timestamp(1_000);
    let client = ContractClient::new(&f.env, &f.contract_id);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &100, &100);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(1), &f.cocoa, &300, &300);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(2), &f.cocoa, &200, &200);
    let record = client.mock_all_auths().finalize_price(&f.cocoa);
    assert_eq!(
        record,
        PriceData {
            price: 200,
            timestamp: 960,
        }
    );
    assert_eq!(client.lastprice(&f.cocoa), Some(record));
}

#[test]
fn test_finalize_price_even_median() {
    let f = setup();
    // Fourth node brings the submission count to an even number, so the
    // median must average the two middle values.
    let extra = Address::generate(&f.env);
    let client = ContractClient::new(&f.env, &f.contract_id);
    client.mock_all_auths().add_node(&f.admin, &extra);
    client.mock_all_auths().set_threshold(&f.admin, &4);
    f.env.ledger().set_timestamp(1_000);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &100, &100);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(1), &f.cocoa, &400, &400);
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(2), &f.cocoa, &200, &200);
    client
        .mock_all_auths()
        .submit_price(&extra, &f.cocoa, &300, &300);
    let record = client.mock_all_auths().finalize_price(&f.cocoa);
    assert_eq!(
        record,
        PriceData {
            price: 250,
            timestamp: 960,
        }
    );
}

#[test]
fn test_finalize_price_clears_pending() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    f.env.ledger().set_timestamp(1_000);
    submit_all(&f, 100);
    client.mock_all_auths().finalize_price(&f.cocoa);
    let pending: Option<Vec<Submission>> = f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .get(&DataKey::Pending(f.cocoa.clone()))
    });
    assert!(pending.is_none());
    // After finalize, nodes may submit again in the same window.
    client
        .mock_all_auths()
        .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &150, &150);
}

#[test]
fn test_finalize_price_prunes_history() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    client.mock_all_auths().set_threshold(&f.admin, &1);
    client.mock_all_auths().set_retention(&f.admin, &2);
    for ts in [1_000u64, 1_100, 1_200] {
        f.env.ledger().set_timestamp(ts);
        client
            .mock_all_auths()
            .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &(ts as i128), &ts);
        client.mock_all_auths().finalize_price(&f.cocoa);
    }
    let prices = client.prices(&f.cocoa, &10).unwrap();
    assert_eq!(prices.len(), 2);
    assert_eq!(prices.get_unchecked(0).timestamp, 1_080);
    assert_eq!(prices.get_unchecked(1).timestamp, 1_200);
}

/// --- SEP-40 read functions ---

#[test]
fn test_base_assets_decimals_resolution() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    assert_eq!(client.base(), asset(&f.env, "USDC"));
    assert_eq!(client.assets(), vec![&f.env, f.cocoa.clone()]);
    assert_eq!(client.decimals(), 7);
    assert_eq!(client.resolution(), 60);
}

#[test]
fn test_price_matches_rounded_window() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    f.env.ledger().set_timestamp(1_000);
    submit_all(&f, 100);
    client.mock_all_auths().finalize_price(&f.cocoa);

    // A query inside the finalized window rounds down to it.
    let res = client.price(&f.cocoa, &1_000);
    assert_eq!(
        res,
        Some(PriceData {
            price: 100,
            timestamp: 960,
        })
    );
    let res = client.price(&f.cocoa, &1_019);
    assert_eq!(
        res,
        Some(PriceData {
            price: 100,
            timestamp: 960,
        })
    );
    // A query in an earlier window with no data returns None.
    let res = client.price(&f.cocoa, &950);
    assert_eq!(res, None);
    // Unknown asset returns None.
    let res = client.price(&asset(&f.env, "COFFEE"), &1_000);
    assert_eq!(res, None);
}

#[test]
fn test_prices_returns_recent_records_oldest_first() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    client.mock_all_auths().set_threshold(&f.admin, &1);
    for (ts, price) in [(1_000u64, 100i128), (1_100, 200), (1_200, 300)] {
        f.env.ledger().set_timestamp(ts);
        client
            .mock_all_auths()
            .submit_price(&f.nodes.get_unchecked(0), &f.cocoa, &price, &ts);
        client.mock_all_auths().finalize_price(&f.cocoa);
    }
    let res = client.prices(&f.cocoa, &2).unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(res.get_unchecked(0).timestamp, 1_080);
    assert_eq!(res.get_unchecked(1).timestamp, 1_200);
    // More records requested than available: all are returned.
    assert_eq!(client.prices(&f.cocoa, &10).unwrap().len(), 3);
    // Degenerate and unknown requests return None.
    assert_eq!(client.prices(&f.cocoa, &0), None);
    assert_eq!(client.prices(&asset(&f.env, "COFFEE"), &2), None);
}

#[test]
fn test_lastprice_returns_most_recent() {
    let f = setup();
    let client = ContractClient::new(&f.env, &f.contract_id);
    assert_eq!(client.lastprice(&f.cocoa), None);
    f.env.ledger().set_timestamp(1_000);
    submit_all(&f, 100);
    client.mock_all_auths().finalize_price(&f.cocoa);
    f.env.ledger().set_timestamp(1_100);
    submit_all(&f, 200);
    client.mock_all_auths().finalize_price(&f.cocoa);
    assert_eq!(
        client.lastprice(&f.cocoa),
        Some(PriceData {
            price: 200,
            timestamp: 1_080,
        })
    );
    assert_eq!(client.lastprice(&asset(&f.env, "COFFEE")), None);
}

/// --- TTL housekeeping ---

#[test]
fn test_extend_instance_ttl_is_permissionless() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    client
        .mock_all_auths()
        .initialize(&admin, &7, &60, &asset(&env, "USDC"));
    // No auth mocking for this call: anyone may extend the TTL.
    client.extend_instance_ttl(&asset(&env, "COCOA"));
}

/// --- events ---

#[test]
fn test_finalize_emits_price_finalized_event() {
    let f = setup();
    f.env.ledger().set_timestamp(1_000);
    submit_all(&f, 100);
    let client = ContractClient::new(&f.env, &f.contract_id);
    client.mock_all_auths().finalize_price(&f.cocoa);
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().unwrap();
    assert_eq!(
        last,
        &PriceFinalized {
            asset: f.cocoa.clone(),
            price: 100,
            timestamp: 960,
        }
        .to_xdr(&f.env, &f.contract_id)
    );
}