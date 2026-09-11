# CI: Clippy job WASM prerequisite fix

## Original failure

The Clippy job in `.github/workflows/ci.yml` ran:

```yaml
- run: cargo clippy --workspace --all-targets -- -D warnings
```

with no preceding setup beyond installing the stable toolchain with the `clippy`
component. This failed to compile because `agripricefloor` depends on a prebuilt
WASM artifact that did not exist in the Clippy job's environment:

```
target/wasm32v1-none/release/agrifeed_oracle.wasm
target/wasm32v1-none/release/agripricefloor.wasm
```

## Exact root cause

`contracts/agripricefloor/src/lib.rs` imports the oracle contract's compiled WASM
directly via `soroban_sdk::contractimport!`:

```rust
pub mod oracle {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/agrifeed_oracle.wasm");
}
```

This macro reads the WASM file from disk **at compile time**. Because
`cargo clippy --all-targets` compiles every target in the workspace (including
`agripricefloor`, which depends on this import), Clippy cannot even finish
compiling without that file already present — independent of any lint outcome.
The Clippy job never built the oracle or pricefloor WASM artifacts, so the
`contractimport!` macro failed to find the file and the whole job failed before
any lint diagnostics were even produced.

## Why Build-and-test already passed

The `build-and-test` job already performed the full prerequisite sequence before
running tests:

1. Install the `wasm32v1-none` Rust target.
2. Install `stellar-cli`.
3. Build the oracle WASM (`stellar contract build --package agrifeed-oracle`).
4. Build the pricefloor WASM (`stellar contract build --package agripricefloor`),
   which itself compiles cleanly because the oracle WASM already exists on disk.
5. Run `cargo test --workspace`, which — like Clippy — compiles the full
   workspace including `agripricefloor`, but by this point the import target
   exists.

The Clippy job simply never carried out steps 1–4, so it lacked the same
prerequisite that already made Build-and-test succeed.

## CI workflow fix

Added the identical prerequisite steps (same target, same install command, same
two `stellar contract build --package <name>` invocations, oracle before
pricefloor) to the Clippy job in `.github/workflows/ci.yml`, immediately before
the unchanged `cargo clippy --workspace --all-targets -- -D warnings` command:

```yaml
clippy:
  name: Clippy
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy
        targets: wasm32v1-none
    - name: Install stellar-cli
      run: curl -fsSL https://github.com/stellar/stellar-cli/raw/main/install.sh | bash
    - name: Build oracle wasm
      run: stellar contract build --package agrifeed-oracle
    - name: Build pricefloor wasm
      run: stellar contract build --package agripricefloor
    - run: cargo clippy --workspace --all-targets -- -D warnings
```

Nothing about the Clippy command itself changed: `--workspace`, `--all-targets`,
and `-D warnings` are all unchanged. The `build-and-test` job was not modified.

## Local validation

Run from `~/agrifeed-contract`:

- `cargo fmt --all -- --check` — no diff, passes.
- `stellar contract build --package agrifeed-oracle` then
  `stellar contract build --package agripricefloor` — both succeed, producing:
  - `target/wasm32v1-none/release/agrifeed_oracle.wasm` (24,031 bytes optimized)
  - `target/wasm32v1-none/release/agripricefloor.wasm` (15,878 bytes optimized)
- `cargo clippy --workspace --all-targets -- -D warnings` — compiles cleanly
  with zero warnings once the artifacts above exist.
- `cargo test --workspace` — 50/50 tests pass (28 in `agrifeed-oracle`, 22 in
  `agripricefloor`), matching the existing baseline exactly; no tests were
  added, removed, or modified.

## GitHub Actions run

Pending push. This section will be updated with the real run ID and final job
results once the change is pushed to `origin/main`.
