use soroban_sdk::contracterror;

/// Errors returned by the agrifeed-oracle contract.
///
/// Every fallible public function returns `Result<T, Error>` except the
/// SEP-40 read functions (`price`, `prices`, `lastprice`), which return
/// `Option` per the SEP-40 spec. A malformed request always produces a typed
/// `Error`, never a raw panic.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// `initialize` was called more than once.
    AlreadyInitialized = 1,
    /// The contract has not been initialized yet.
    NotInitialized = 2,
    /// The caller is not the contract administrator.
    NotAdmin = 3,
    /// The caller is not an authorized price node.
    NotAuthorizedNode = 4,
    /// The asset is not tracked as a commodity.
    UnknownCommodity = 5,
    /// The commodity is already tracked.
    CommodityAlreadyExists = 6,
    /// Fewer pending submissions than the configured threshold.
    ThresholdNotMet = 7,
    /// There are no pending submissions to finalize.
    NoPendingSubmissions = 8,
    /// The node already has a pending submission in the current window.
    DuplicateSubmission = 9,
    /// The submitted price is not positive, or internal price arithmetic
    /// overflowed (which cannot occur for well-formed data).
    InvalidPrice = 10,
    /// The threshold is zero or exceeds the number of nodes. Also returned by
    /// `initialize` when `resolution` is zero and by `set_retention` when
    /// `retention` is zero, since both are configuration values that must be
    /// positive.
    InvalidThreshold = 11,
}
