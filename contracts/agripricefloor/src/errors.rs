use soroban_sdk::contracterror;

/// Errors returned by the agripricefloor contract.
///
/// Every fallible public function returns `Result<T, Error>`. A malformed
/// request always produces a typed `Error`, never a raw panic.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// `initialize` was called more than once.
    AlreadyInitialized = 1,
    /// `settle` was called but the buyer never funded the agreement.
    NotFunded = 2,
    /// `fund` was called after the agreement was already funded.
    AlreadyFunded = 3,
    /// The agreement was already settled.
    AlreadySettled = 4,
    /// `settle` was called before the maturity timestamp.
    NotYetMature = 5,
    /// The oracle has no price for the commodity, so no settlement price can
    /// be fabricated.
    OracleDataUnavailable = 6,
    /// `cancel` was called before the applicable grace period elapsed.
    GracePeriodNotElapsed = 7,
    /// The caller is not the party the function requires: `fund` must be
    /// called by the buyer and `cancel` by the farmer or the buyer.
    Unauthorized = 8,
    /// The funding amount is not positive.
    InvalidAmount = 9,
    /// Internal payout arithmetic overflowed, which cannot occur for
    /// well-formed prices and notional values.
    ArithmeticOverflow = 10,
}
