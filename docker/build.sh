#!/usr/bin/env bash

cd crates/server

args=("--release")

if [[ "$TESTNET" -eq "1" ]]; then
    args+=("--features=testnet")
fi

set -x
cargo build ${args[@]}
