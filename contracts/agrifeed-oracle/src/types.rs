use soroban_sdk::{contracttype, Address, Symbol};

/// A single price point for an asset, as defined by SEP-40.
///
/// `price` is expressed in units of `1 / 10^decimals` of the base asset.
/// `timestamp` is a Unix timestamp in seconds, rounded down to the contract's
/// resolution window.
#[contracttype]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

/// The asset a price refers to, per SEP-40: a Stellar asset (identified by
/// its asset contract address) or any other asset (identified by a symbol).
///
/// Agricultural commodities are represented as `Asset::Other(Symbol)`, for
/// example `Asset::Other(Symbol::new(&env, "COCOA"))`.
#[contracttype]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

/// Storage keys for the oracle contract.
///
/// Admin-level configuration lives in instance storage. Per-commodity data
/// lives in persistent storage, keyed independently, because it can grow and
/// needs an explicitly extended TTL under Soroban's storage model.
#[contracttype]
pub enum DataKey {
    /// Address of the contract administrator. Instance storage.
    Admin,
    /// Addresses of the authorized price nodes. Instance storage.
    Nodes,
    /// Minimum number of pending submissions required to finalize a price.
    /// Instance storage.
    Threshold,
    /// Number of decimals used to represent prices. Instance storage.
    Decimals,
    /// Length of one price window in seconds. Instance storage.
    Resolution,
    /// The base asset of this feed, per SEP-40. Instance storage.
    BaseAsset,
    /// The list of tracked commodities. Instance storage.
    Commodities,
    /// Maximum number of finalized records kept per commodity. Instance
    /// storage. Defaults to [`crate::DEFAULT_RETENTION`].
    Retention,
    /// Pending submissions for an asset, cleared on finalize. Persistent
    /// storage.
    Pending(Asset),
    /// Finalized price history for an asset, bounded by the retention limit.
    /// Persistent storage.
    History(Asset),
}

/// A single price submission from one node for one asset.
#[contracttype]
pub struct Submission {
    /// Address of the submitting node.
    pub node: Address,
    /// Submitted price in units of `1 / 10^decimals` of the base asset.
    pub price: i128,
    /// Unix timestamp, in seconds, at which the node observed the price.
    pub source_ts: u64,
    /// Ledger timestamp, in seconds, at which the submission was recorded.
    pub submitted_at: u64,
}