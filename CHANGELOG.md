# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `agrifeed-oracle`: a SEP-40 compliant, multi-node price oracle with
  admin-managed nodes, commodities, and finalize threshold.
- `agripricefloor`: a cash-settled price-floor agreement contract that reads
  the oracle and pays the farmer the shortfall when the market price is below
  the agreed floor at maturity.
- Cross-contract integration test exercising both deployed contracts end to
  end.
- GitHub Actions CI running rustfmt, clippy, contract wasm builds, and the
  full test suite.