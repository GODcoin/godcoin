use sodiumoxide::crypto::hash::sha256;
use std::{
    fmt::{self, Debug, Formatter},
    ops::Deref,
};

pub mod error;
pub mod key;
pub mod script_hash;
pub mod sigpair;

pub use self::error::*;
pub use self::key::*;
pub use self::script_hash::*;
pub use self::sigpair::*;

#[derive(Clone, PartialEq)]
pub struct Digest(sha256::Digest);

impl Digest {
    #[inline]
    pub fn from_slice(slice: &[u8]) -> Option<Digest> {
        Some(Digest(sha256::Digest::from_slice(slice)?))
    }
}

impl Deref for Digest {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for Digest {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Debug for Digest {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("Digest(\"")?;
        for x in self.as_ref() {
            write!(f, "{:x}", x)?;
        }
        f.write_str("\")")
    }
}

#[inline]
pub fn double_sha256(buf: &[u8]) -> Digest {
    Digest(sha256::hash(sha256::hash(buf).as_ref()))
}
