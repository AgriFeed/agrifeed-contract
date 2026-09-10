//! Storage keys for the agripricefloor contract.

use soroban_sdk::{contracttype, Address, Symbol};

/// The asset a price refers to, per SEP-40: a Stellar asset (identified by
/// its asset contract address) or any other asset (identified by a symbol).
///
/// This mirrors `agrifeed_oracle::Asset` variant-for-variant, so the two
/// types share the same XDR encoding. It is declared locally, rather than
/// reused from the oracle's `contractimport!`-generated client module,
/// because a type only pulled in through an import is not written into
/// this contract's own on-chain spec: any caller that builds calls from
/// this contract's spec (the CLI's implicit help, or generated client
/// bindings) would otherwise have no way to learn its shape.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

/// Storage keys for the price-floor agreement.
///
/// All entries are small, always-loaded configuration, so they live in
/// instance storage.
#[contracttype]
pub enum DataKey {
    /// Address of the farmer (the protected party). Instance storage.
    Farmer,
    /// Address of the buyer (the collateral provider). Instance storage.
    Buyer,
    /// The commodity the agreement references, as reported by the oracle.
    /// Instance storage.
    Commodity,
    /// The guaranteed minimum price per unit of commodity, in the oracle's
    /// price units. Instance storage.
    FloorPrice,
    /// The notional quantity of commodity covered by the agreement. Instance
    /// storage.
    Notional,
    /// Address of the settlement token contract used for collateral and
    /// payout. Instance storage.
    SettlementToken,
    /// Unix timestamp, in seconds, at which the agreement matures. Instance
    /// storage.
    Maturity,
    /// Address of the SEP-40 oracle contract that prices the commodity.
    /// Instance storage.
    Oracle,
    /// Whether the buyer has deposited the collateral. Instance storage.
    Funded,
    /// Whether the agreement has been settled. Instance storage.
    Settled,
    /// Whether `cancel` has been called successfully at least once.
    /// Instance storage. Set to `true` unconditionally by `cancel`, in both
    /// the funded and unfunded branches; never read or written anywhere
    /// else. Added specifically so a point-in-time storage read can tell
    /// "cancelled" apart from every other state without depending on
    /// unbroken event history (see the `Cancelled` event's own doc comment
    /// for the gap this closes, and why it could not be closed for any
    /// already-deployed instance).
    Cancelled,
}
