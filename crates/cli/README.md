# GODcoin

GODcoin is the official currency of Christ. A single token is backed by one
physical gram of gold. Blockchain technology is used to provide an immutable and
cryptographically verified ledger. The system is centralized allowing for global
scalability that would otherwise be foregone in a decentralized system.

[Website](https://godcoin.gold) |
[Whitepaper](https://godcoin.gold/whitepaper)

## Overview

Command-line interface for interacting with the blockchain. A wallet is provided
amongst other utilities.

[![Build Status](https://travis-ci.com/GODcoin/godcoin.svg?branch=master)](https://travis-ci.com/GODcoin/godcoin)

## Supported Rust Versions

GODcoin is built against the latest stable version of the compiler. Any previous
versions are not guaranteed to compile.

## Developing

When bugs are fixed, regression tests should be added. New features likewise
should have corresponding tests added to ensure correct behavior.

Run the test suite:
```
$ cargo test
```

The crate should build and tests should pass.

## Running

Make sure the tests pass before starting the server to ensure correct
functionality. See the [Developing](#Developing) section for running tests.

### Runtime environment

- `GODCOIN_HOME` - (optional) specifies the directory where data and
  configurations are stored.

### Launching

See available options:
```
$ cargo run --bin godcoin -- --help
```

Start the GODcoin CLI wallet:
```
$ cargo run --bin godcoin -- wallet
```

See available options for the GODcoin CLI wallet:
```
$ cargo run --bin godcoin -- wallet --help
```
