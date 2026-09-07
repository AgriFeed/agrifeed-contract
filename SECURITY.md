# Security

## Reporting a vulnerability

Please do not open a public issue for a security vulnerability. Report it privately by
emailing the maintainers at security@agrifeed.example (placeholder, replace with the real
address) or by opening a private security advisory on GitHub (if enabled for this
repository).

Include, when possible:

- the contract and function affected,
- a description of the vulnerability and its impact,
- a minimal reproduction or test case,
- your suggested fix, if you have one.

You should receive an acknowledgement within a few business days, followed by updates as
the issue is triaged and fixed. Please allow time for a fix to be prepared before the
issue is disclosed publicly.

## Scope and threat model

This workspace contains example and reference code for the Soroban contracts on the
Stellar network. It does not itself move real funds in production.

The intended trust model is:

- The **oracle admin** controls the node set, the threshold, and the tracked commodities.
  A compromised or malicious admin can change which addresses submit prices and can add or
  remove commodities. The admin cannot directly set prices.
- The **price nodes** are trusted to report honest observations. The threshold requires
  more than one node to agree before a price is finalized, so a single node cannot move a
  price alone. Anyone can finalize, but only after the configured threshold of
  submissions exists.
- A **pricefloor agreement** is only as safe as the oracle it reads and the settlement
  token it holds. The consumer contract never fabricates a settlement price: when the
  oracle has no price it fails with a typed error and the funded collateral can be
  recovered by cancellation after the grace windows described in the README.
- The contracts hold settlement token collateral between funding and settlement or
  cancellation. Admin keys and node keys must be protected accordingly.

The oracle and pricefloor contracts are an engineering example, not yet audited. Do not
deploy them with real value until they have been reviewed by security professionals
familiar with the Soroban runtime.

## Coding guarantees

The codebase follows these rules by design:

- No `panic!`, `unwrap`, or `expect` outside test code. Every fallible path returns a
  typed `Error` or `Option`.
- No floating point. All price math uses checked `i128` arithmetic.
- Every state-mutating function requires the correct authorization before mutating state
  and emits an event.
- Every persistent storage write extends the affected entry's TTL in the same call.

If you find a violation of these guarantees in non-test code, report it as a security
issue.

## Supported versions

Only the latest `main` branch is supported. The contracts are versioned as a workspace;
fixes land on `main` and are tagged with releases when they are published.
