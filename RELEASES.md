## Releases

All crates must have the same version number when creating a release. This
simplifies documenting any changes.

# Unreleased

- Allow building with the testnet feature flag. Tests are by default built with
  the testnet feature flag.
- The testnet feature flag changes the global asset constant symbol to TEST. The
  recommendation for library authors is to do the same when testing against a
  testnet.

### Breaking changes

- All times now deal with seconds instead of milliseconds.
- Transactions no longer have a "timestamp" field in place of a new "expiry"
  field. This is a quality of life improvement as the transaction can be
  broadcasted at any time up until the expiry. Previously, users would have to
  broadcast a transaction at an exact time for acceptance, which isn't practical
  when dealing with multiple signing parties.
- Transactions now have a nonce field. This field must be unique when all other
  transaction data is the same. In other words, the nonce can be reused whenever
  other data (e.g. timestamp) chances.
- TxId hashes now include a chain ID and no longer include signatures.
- Transactions are now signed using the TxID.

# Version 0.3.0 (2019-12-31)

- Send heartbeat pings to clients to detect dead connections and close them.
  Pings are implemented at the application protocol level (i.e not WebSockets).
- The wallet will now explicitly check for errors and responses before returning
  when sending a request.
- Max value ID requests will no longer respond with an IO error. However,
  applications may misbehave as this is the reserved ID for general messages.

### Breaking Changes

- The network binary protocol now has a body type signifying the type of
  message. By decoupling from RPC, we can now send generic messages that don't
  expect a response.

# Version 0.2.1 (2019-12-03)

Fixes a memory issue when retrieving block ranges by ensuring that the network
handler can't queue an infinite amount of messages.

- Configure the max message send queue in the WebSocket handler for back
  pressure control.

# Version 0.2.0 (2019-11-29)

This release improves the networking protocol and allows eliminating an
unnecessary round-trip RPC call when synchronizing blocks.

- Network protocol supports setting the block filter with an empty hash set to
  retrieve only block headers.
- Add ClearBlockFilter net API to the network protocol.
- Remove GetBlockHeader net API from the network protocol.
- Add GetFullBlock net API to allow filtering all blocks and allow retrieving
  full blocks when necessary.
- Add GetBlockRange net API to stream back a range of blocks to the client. This
  is more efficient than naively looping GetBlock requests to the server.
- Use a bounded sender when sending network messages for back pressure control.

### Breaking changes

- The network protocol message type constants have been changed.
- Clients must support streaming responses from the GetBlockRange network API.

# Version 0.1.0 (2019-11-14)

This marks the first release of the project. The blockchain server supports
running an alpha network. Clients are able to connect to the server and be able
to interact with the blockchain using the CLI wallet.

Power users will be using the CLI crate to interact with the public network when
it launches. Developers can take a look at `crates/godcoin` for creating
applications and `crates/server` for running a private alpha network locally for
testing.

### Crates released
- crates/cli
- crates/godcoin
- crates/server
