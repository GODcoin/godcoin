## Releases

All crates must have the same version number when creating a release. This
simplifies documenting any changes.

# Unreleased

- Network protocol supports setting the block filter with an empty hash set to
  retrieve only block headers.
- Add ClearBlockFilter net API to the network protocol.
- Remove GetBlockHeader net API from the network protocol.
- Add GetFullBlock net API to allow filtering all blocks and allow retrieving
  full blocks when necessary.

### Breaking changes

- The network protocol message type constants have been changed.

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
