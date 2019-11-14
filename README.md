# GODcoin

GODcoin is the official currency of Christ. A single token is backed by one
physical gram of gold. Blockchain technology is used to provide an immutable and
cryptographically verified ledger. The system is centralized allowing for global
scalability that would otherwise be foregone in a decentralized system.

[Website](https://godcoin.gold) |
[Whitepaper](https://godcoin.gold/whitepaper)

## Overview

This repository provides GODcoin's core software and library implementations and
is to be used as a point of reference when developing software in other
languages.

[![Build Status](https://travis-ci.com/GODcoin/godcoin.svg?branch=master)](https://travis-ci.com/GODcoin/godcoin)

## Supported Rust Versions

GODcoin is built against the latest stable version of the compiler. Any previous
versions are not guaranteed to compile.

## Project Layout

Each crate lives under the `crates` directory. Developers looking to use GODcoin
in their software will want the library under `crates/godcoin`.

- `crates/cli`: Provides a CLI for the wallet and other utilities.
- `crates/godcoin`: Core GODcoin library.
- `crates/server`: Core GODcoin server daemon.
