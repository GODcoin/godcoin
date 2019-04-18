use sodiumoxide::crypto::hash::sha256;

pub mod error;
pub mod key;
pub mod script_hash;
pub mod sigpair;

pub use self::error::*;
pub use self::key::*;
pub use self::script_hash::*;
pub use self::sigpair::*;

#[inline]
pub fn double_sha256(buf: &[u8]) -> sha256::Digest {
    sha256::hash(sha256::hash(buf).as_ref())
}
