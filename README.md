# GODcoin
[![Build Status](https://travis-ci.com/GODcoin/godcoin-rs.svg?branch=master)](https://travis-ci.com/GODcoin/godcoin-rs)

https://godcoin.gold

## What is GODcoin?

GODcoin is the official currency of Christ. GODcoin is backed by physical gold
assets. The digital token name is represented as GRAEL. A single token will be
represented by a gram of gold.

For more information see the [whitepaper](https://godcoin.gold/whitepaper).

## Development

This project is still under heavy development. APIs are currently evolving all
the time and not to be considered stable. It is possible to run a private test
net for personal use and development.

### Prerequisites

Ensure you have the following software installed:

- Rust compiler (1.36+)
- libsodium

### Getting started

Make sure the source code is locally available by either cloning the repository
or downloading it.

#### Runtime environment

- `GODCOIN_HOME` - specifies the directory where data and configurations are
  stored.

#### Running

Run the test suite:
```
$ cargo test
```

Launch GODcoin CLI:
```
$ cargo run --bin godcoin-cli
```

Launch GODcoin server:
```
$ cargo run --bin godcoin-server
```

The server requires a configuration file in the home folder called
`config.toml`

Configuration keys:

- `minter_key` - (required) Minter key to use for block production
- `bind_address` - (optional - default is 127.0.0.1:7777) The bind address for
  the server to listen on
