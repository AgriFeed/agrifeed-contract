# Contributing

Thanks for your interest in agrifeed-contract. This document covers how to set up a
development environment, the standards the code follows, and how to get changes merged.

## Development setup

Install the stable Rust toolchain with the `wasm32v1-none` target and `stellar-cli`:

```bash
rustup target add wasm32v1-none
curl -fsSL https://github.com/stellar/stellar-cli/raw/main/install.sh | bash
```

The build order matters. `agripricefloor` compiles against the oracle's deployed
interface: its client and the shared `Asset` type are generated from the oracle wasm at
compile time with `contractimport!`, so the oracle wasm must exist first.

```bash
stellar contract build --package agrifeed-oracle
stellar contract build --package agripricefloor
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

A fresh checkout needs the two `stellar contract build` steps before `cargo test` will
compile, because the tests also read the wasm files.

## Coding standards

- Rust edition 2021, stable toolchain, `soroban-sdk` pinned in `Cargo.toml`.
- `snake_case` for functions and variables, `PascalCase` for types.
- No `panic!`, `unwrap`, or `expect` outside `#[cfg(test)]` code. Every fallible path
  returns a typed `Error` or `Option`.
- No floating point. All price math uses `i128` with checked operations; a checked
  operation that returns `None` propagates a typed error.
- Every state-mutating function checks its required authorization before mutating
  anything, and emits an event.
- Every persistent storage write extends the affected entry's TTL in the same call.
- Doc comments on every public function explaining arguments, the return value, and the
  failure conditions. Do not use the em dash character in docs or prose.
- Public function signatures that implement a specification (SEP-40, the contract design
  in the README) must not be changed to satisfy style lints.

## Making changes

- Create a branch from `main` for your work.
- Keep commits small and focused: one function, one type, or one test block per commit.
  Do not batch unrelated changes.
- Use Conventional Commits: `type(scope): description`, for example
  `fix(oracle): reject zero-price submissions`.
- Verify locally before pushing: `cargo fmt --all`, `cargo clippy --workspace
  --all-targets -- -D warnings`, the wasm builds above, and `cargo test --workspace`.

## Tests

- Unit tests live in `src/test.rs` inside each contract crate.
- The cross-contract flow lives in `tests/integration.rs` and exercises both deployed
  wasms end to end.
- Add a test for every new failure path, not just the happy path.

## Reporting issues

Open an issue for bugs and feature requests. For security vulnerabilities, follow
[SECURITY.md](SECURITY.md).
