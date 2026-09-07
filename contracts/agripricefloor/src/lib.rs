//! agripricefloor
//!
//! An example consumer of the agrifeed-oracle SEP-40 price feed. It settles
//! a cash-settled price-floor agreement between a farmer and a buyer: if the
//! market price of a commodity falls below the agreed floor at maturity, the
//! buyer pays the farmer the difference; otherwise the buyer's collateral is
//! refunded. No physical delivery and no custody of goods.
#![no_std]

use soroban_sdk::{contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {}