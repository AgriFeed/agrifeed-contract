//! Storage keys for the agripricefloor contract.

use soroban_sdk::contracttype;

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
    /// Ledger timestamp at which `settle` last failed because the oracle had
    /// no price. Zero when no such failure occurred. Instance storage.
    SettleFailedAt,
}