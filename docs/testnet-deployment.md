# Testnet Deployment

Deploys `agrifeed-oracle`, deploys `agripricefloor`, wires them together, and proves
the whole thing end to end with one demo price-floor deal. Every command below has been
run against Stellar testnet; where a function argument's exact encoding isn't obvious,
check `--help` on the live contract rather than guessing, the CLI derives that help from
the contract's own on-chain spec.

See [CONTRIBUTING.md](../CONTRIBUTING.md#known-build-quirks) for build quirks
(`cargo clean` not clearing `wasm32v1-none` output, the workspace-root build failure,
and CLI indexing lag) that apply throughout this guide.

## 0. Preflight

```bash
stellar --version
stellar network use testnet
```

Needs `stellar-cli` 27.x or newer.

## 1. Create identities

```bash
stellar keys generate deployer --network testnet --fund
stellar keys generate relayer-node --network testnet --fund
stellar keys address deployer
stellar keys address relayer-node
```

`--fund` pulls testnet XLM from Friendbot automatically. `relayer-node`'s address is
what gets passed to `add_node`; its secret (`stellar keys show relayer-node`) is what a
relayer service's `NODE_RELAYER_SECRET_KEY` needs.

## 2. Build both contracts

```bash
stellar contract build --package agrifeed-oracle
stellar contract build --package agripricefloor
```

Build each package explicitly. Bare `stellar contract build` (no `--package`) fails at
the workspace root, see CONTRIBUTING.md. Confirm both wasm files exist:

```bash
ls target/wasm32v1-none/release/*.wasm
```

## 3. Deploy the oracle

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/agrifeed_oracle.wasm \
  --source-account deployer \
  --network testnet \
  --alias agrifeed-oracle
```

Prints a contract ID. Save it as `ORACLE_ID`.

## 4. Check the initialize signature before calling it

```bash
stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  initialize --help
```

`--source-account` is required even for `--help`, on a bare `contract invoke` it's a
required flag regardless of whether the call actually sends a transaction.

This prints the expected argument shapes, including the `Asset` enum. The CLI's own
`--help` text renders the example as unquoted `{Other:hello}`, but the argument parser
actually expects **quoted JSON**: `{"Other":"COCOA"}`, not `{Other:COCOA}`. Use the
quoted form.

## 5. Initialize the oracle

```bash
stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  initialize --admin <deployer address> --decimals 7 --resolution 3600 \
  --base_asset '{"Other":"USDC"}'
```

7 decimals and `Other("USDC")` as the base asset match the test suite's convention
(`contracts/agrifeed-oracle/src/test.rs`), not a spec requirement. `resolution` is the
price window in seconds; match it to the relayer's submission interval.

## 6. Add the relayer node and set the threshold

```bash
stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  add_node --admin <deployer address> --node <relayer-node address>

stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  set_threshold --admin <deployer address> --threshold 1
```

Threshold of 1 is only correct for a single-node testnet validation pass, there is
nothing to take a median of yet. Raise it the moment a second independent node is
added.

## 7. Add tracked commodities

```bash
stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  add_commodity --admin <deployer address> --asset '{"Other":"COCOA"}'
```

Repeat per commodity, checking the exact `Asset` format the same way as step 4. Only
add a commodity a relayer can actually supply data for, an oracle that never finalizes
a price for a listed asset is worse than one that doesn't list it.

## 8. Deploy the price-floor contract, get its wasm hash

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/agripricefloor.wasm \
  --source-account deployer \
  --network testnet \
  --alias agripricefloor-template
```

Uploads the wasm once and reports its hash. Save it as `PRICEFLOOR_WASM_HASH`, every
actual deal is a new instance deployed from this hash via `--wasm-hash`, not a shared
singleton, since each deal has its own farmer, buyer, and terms.

Verify the spec resolved cleanly before relying on it, in particular that `Asset` is
defined natively rather than merely referenced (see the "Asset type" note below):

```bash
stellar contract info --wasm target/wasm32v1-none/release/agripricefloor.wasm
```

(`stellar contract inspect` is the same command under its old, deprecated name, it
prints a deprecation warning and will be removed in a future CLI release, use `info`.)

### Note: the `Asset` type

`agripricefloor` declares its own local `Asset` type (`contracts/agripricefloor/src/types.rs`)
rather than reusing the oracle's via `contractimport!`. A type only pulled in through an
import is not written into the importing contract's own on-chain spec, so any caller that
builds calls from that spec, the CLI's implicit `--help`, or generated client bindings,
has no way to learn its shape. The two types are declared with identical variants
(`Stellar(Address)`, `Other(Symbol)`), so they share the same XDR encoding; a small
conversion function moves between them at the oracle-call boundary. If you see a
`Missing Entry Asset` error invoking `agripricefloor`, first confirm you're on a wasm
built after this fix (`stellar contract info` should list `Union: Asset` with both cases,
not just a reference to it).

## 9. Create identities for the demo deal

```bash
stellar keys generate demo-farmer --network testnet --fund
stellar keys generate demo-buyer --network testnet --fund
```

You also need a settlement token address. Don't hardcode a testnet USDC issuer from
memory, it can go stale. The simplest reliable option is native XLM's Stellar Asset
Contract, deterministic and requires no setup since Friendbot-funded accounts already
hold a balance:

```bash
stellar contract id asset --asset native --network testnet
```

Use a real issued/wrapped asset instead if the deal specifically needs USDC-denominated
collateral.

## 10. Deploy and initialize one demo instance

```bash
stellar contract deploy \
  --wasm-hash <hash from step 8> \
  --source-account deployer \
  --network testnet \
  --alias demo-pricefloor-1

stellar contract invoke --id demo-pricefloor-1 --source-account deployer --network testnet -- \
  initialize --help
```

Then initialize with real values: a maturity timestamp a few minutes out for testing, a
small notional, and the oracle's `ORACLE_ID` from step 3.

```bash
stellar contract invoke --id demo-pricefloor-1 --source-account demo-farmer --network testnet -- \
  initialize \
  --farmer <demo-farmer address> \
  --buyer <demo-buyer address> \
  --commodity '{"Other":"COCOA"}' \
  --floor_price 1000 \
  --notional 10 \
  --settlement_token <settlement token address> \
  --maturity_ts <unix timestamp a few minutes out> \
  --oracle <ORACLE_ID>
```

`initialize` requires both `farmer.require_auth()` and `buyer.require_auth()`. If both
keys are held locally (as they are here, generated in step 9), invoking with either
party as `--source-account` is enough, the CLI auto-signs the other required auth entry
from the matching local key. In a real deployment where farmer and buyer are different
people, this needs an offline multi-party signing flow instead.

Then fund it:

```bash
stellar contract invoke --id demo-pricefloor-1 --source-account demo-buyer --network testnet -- \
  fund --buyer <demo-buyer address> --amount <collateral amount, in the settlement token's smallest unit>
```

## 11. Prove the whole thing end to end

```bash
stellar contract invoke --id agrifeed-oracle --source-account relayer-node --network testnet -- \
  submit_price --node <relayer-node address> --asset '{"Other":"COCOA"}' --price <value> \
  --source_ts <unix timestamp>

stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  finalize_price --asset '{"Other":"COCOA"}'

stellar contract invoke --id agrifeed-oracle --source-account deployer --network testnet -- \
  lastprice --asset '{"Other":"COCOA"}'
```

`lastprice` should now return a real `PriceData`, not `None`. Once `maturity_ts` has
passed, settle:

```bash
stellar contract invoke --id demo-pricefloor-1 --source-account deployer --network testnet -- \
  settle
```

`settle` is permissionless, any account can pay the fee to trigger it once the
agreement is funded and mature.

## 12. Generate client bindings (optional, proves the fix for external callers)

```bash
stellar contract bindings typescript \
  --contract-id <ORACLE_ID> \
  --network testnet \
  --output-dir packages/sdk/generated/oracle \
  --overwrite

stellar contract bindings typescript \
  --contract-id <a deployed pricefloor instance ID> \
  --network testnet \
  --output-dir packages/sdk/generated/pricefloor \
  --overwrite
```

A clean generation with no `Missing Entry` error, and a generated `Asset` type in the
output, is the real proof that an external caller (a frontend, an indexer) can build
valid calls against the deployed contract, not just that the contract's own test suite
passes.

## Values to save

```
ORACLE_ID:
PRICEFLOOR_WASM_HASH:
DEPLOYER_ADDRESS:
RELAYER_NODE_ADDRESS:
DEMO_PRICEFLOOR_INSTANCE_ID:
DEMO_FARMER_ADDRESS:
DEMO_BUYER_ADDRESS:
SETTLEMENT_TOKEN_ADDRESS:
```

For a consuming frontend/indexer:

```
NEXT_PUBLIC_STELLAR_NETWORK=testnet
NEXT_PUBLIC_SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
NEXT_PUBLIC_ORACLE_CONTRACT_ID=<ORACLE_ID>
NEXT_PUBLIC_PRICEFLOOR_WASM_HASH=<PRICEFLOOR_WASM_HASH>
NODE_RELAYER_SECRET_KEY=<relayer-node's secret, from step 1, never commit this>
```
