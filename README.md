# agrifeed-contract

A pure Rust [Soroban](https://developers.stellar.org/docs/build/smart-contracts) workspace
for cash-settled agricultural commodity price protection. It contains two contracts:

1. **`agrifeed-oracle`**: a SEP-40 compliant, multi-node price oracle for agricultural
   commodity prices (cocoa, coffee, cashew, cotton, maize).
2. **`agripricefloor`**: an example consumer that reads the oracle and settles a
   cash-settled price-floor agreement between a farmer and a buyer. There is no physical
   delivery and no custody of goods, only token payouts.

## Repository layout

```
agrifeed-contract/
├── Cargo.toml                  # workspace root
├── contracts/
│   ├── agrifeed-oracle/        # SEP-40 price feed aggregator
│   └── agripricefloor/         # price-floor agreement consumer
└── tests/
    └── integration.rs          # full flow across both contracts
```

## Prerequisites

- Stable Rust with the `wasm32v1-none` target: `rustup target add wasm32v1-none`
- [stellar-cli](https://github.com/stellar/stellar-cli) for contract builds

The contracts pin `soroban-sdk = "27.0.6"`. Contracts must be built for the
`wasm32v1-none` target with `stellar contract build`, never with a bare
`cargo build`.

## Building

Build the oracle first, then the pricefloor contract. `agripricefloor` compiles against
the oracle's deployed interface: its client and the shared `Asset` type are generated at
compile time from the oracle wasm via `contractimport!`, so the oracle wasm must exist on
disk before the pricefloor crate (or the integration tests) compile.

```bash
stellar contract build --package agrifeed-oracle
stellar contract build --package agripricefloor
```

The wasms land in `target/wasm32v1-none/release/`.

## Testing

Unit tests live inside each contract crate; the integration test is at `tests/integration.rs`.
Run everything from the workspace root with the wasms already built:

```bash
cargo test --workspace
```

Because `contractimport!` reads wasm files at compile time, a fresh checkout needs the two
`stellar contract build` steps above before the tests compile.

## agrifeed-oracle

A SEP-40 compliant price feed aggregator. Authorized nodes submit price observations for
tracked commodities; a permissionless finalize step aggregates the pending submissions of a
resolution window into a single median price.

### Storage model

- Admin configuration (admin address, node list, threshold, decimals, resolution, base
  asset, commodity list, retention limit) lives in **instance storage**.
- Per-commodity data (`Pending(asset)` submissions and `History(asset)` finalized records)
  lives in **persistent storage**, keyed independently, with an explicit TTL extension on
  every write.

### Admin and node management

| Function | Auth | Behavior |
|---|---|---|
| `initialize(admin, decimals, resolution, base_asset)` | `admin` | One-time setup. Rejects a second call with `Error::AlreadyInitialized`. |
| `add_node(admin, node)` | `admin` | Adds a price node. Adding an existing node is a no-op. |
| `remove_node(admin, node)` | `admin` | Removes a price node. Does not retroactively invalidate finalized prices. |
| `set_threshold(admin, threshold)` | `admin` | Requires `1 <= threshold <= node count`, else `Error::InvalidThreshold`. |
| `set_retention(admin, retention)` | `admin` | Sets the per-commodity history retention limit (default 90). |
| `add_commodity(admin, asset)` | `admin` | Tracks a new commodity. Duplicates return `Error::CommodityAlreadyExists`. |

### Price ingestion

| Function | Auth | Behavior |
|---|---|---|
| `submit_price(node, asset, price, source_ts)` | `node` | Validates the node, commodity, and a positive price, and rejects a second pending submission from the same node in the current resolution window (`Error::DuplicateSubmission`). |
| `finalize_price(asset)` | none | Computes the integer median of the pending submissions (the average of the two middle values, via checked arithmetic, when the count is even), appends a `PriceData` timestamped `floor(now / resolution) * resolution`, prunes history to the retention limit, and clears the pending submissions. Below the configured threshold it returns `Error::ThresholdNotMet`; a threshold of zero can never be met, so a single node can never move a price alone. |

### SEP-40 interface

`base`, `assets`, `decimals`, `resolution`, `price`, `prices`, and `lastprice` are
implemented verbatim from
[SEP-40](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0040.md).
Commodities are `Asset::Other(Symbol)`, for example `Asset::Other(Symbol::new(&env,
"COCOA"))`. Per the spec, these reads never throw for unknown assets or out-of-range
timestamps; they return `None` so consumers handle the error themselves.

- `price(asset, timestamp)` rounds the query timestamp down to the resolution window and
  returns that window's record, or `None`.
- `prices(asset, records)` returns up to `records` most recent records, ordered oldest
  first. Zero records returns `None`.
- `lastprice(asset)` returns the most recent record, or `None`.

### TTL housekeeping

`extend_instance_ttl(asset)` is permissionless and extends the instance storage plus the
pending and history entries for `asset` when they exist. Relayers should call it
periodically (for example from a cron job) so the oracle's configuration is never archived
from inactivity.

### Events

Every state-mutating function emits an event: `Initialized`, `NodeAdded`, `NodeRemoved`,
`ThresholdUpdated`, `RetentionUpdated`, `CommodityAdded`, `PriceSubmitted`, and
`PriceFinalized` (which carries the asset, price, and rounded timestamp).

## agripricefloor

A cash-settled price-floor agreement. Both parties sign the terms at creation, the buyer
deposits collateral in the settlement token, and after maturity anyone may settle the
agreement against the oracle's last reported price.

| Function | Auth | Behavior |
|---|---|---|
| `initialize(farmer, buyer, commodity, floor_price, notional, settlement_token, maturity_ts, oracle)` | `farmer` and `buyer` | Stores the terms. Rejects a second call with `Error::AlreadyInitialized`. |
| `fund(buyer, amount)` | `buyer` | Transfers `amount` of the settlement token into the contract. Rejects repeat funding (`Error::AlreadyFunded`) and non-positive amounts (`Error::InvalidAmount`). |
| `settle()` | none | Maturity and funding checks first, then reads `oracle.lastprice(commodity)`. If the oracle has no price it returns `Error::OracleDataUnavailable` and never fabricates a payout. Below the floor, the farmer receives `min((floor_price - market_price) * notional, funded_amount)` and the buyer the remainder; at or above the floor, the buyer receives the full collateral back. |
| `cancel(caller)` | `farmer` or `buyer` | An unfunded agreement can be cancelled after maturity plus a 48-hour grace deadline. A funded agreement can be cancelled once maturity plus a further 48-hour grace window has passed while the oracle still has no price (so settlement keeps failing); the full collateral is then refunded to the buyer. If the oracle has a price, the agreement settles instead and cancel is refused. |

The funded collateral is measured as the contract's settlement token balance at settle
time; the contract holds no other funds. All payout math uses checked `i128` arithmetic.

> Note on cancel semantics: Soroban rolls back every state change of a failed invocation,
> so a `settle` that returns `Error::OracleDataUnavailable` cannot persist a marker of its
> failure. Instead of recording attempts, `cancel` checks the oracle directly and allows a
> funded refund once the further grace window has elapsed while the oracle is still silent.

### Events

`Initialized`, `Funded` (amount), `Settled` (payout and market price used), and
`Cancelled`.

## Deploying the oracle

`agripricefloor` never hardcodes a contract address. Deploy the oracle first, then pass its
address (alongside the token and both parties) to `initialize`.

## License

Apache-2.0. See [LICENSE](LICENSE).
