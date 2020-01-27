use sodiumoxide::crypto::{hash::sha256, sign};
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

pub const DIGEST_BYTES: usize = sha256::DIGESTBYTES;

macro_rules! impl_wrapper {
    ($name:ident, $wrapper:ty) => {
        #[repr(transparent)]
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub $wrapper);

        impl $name {
            #[inline]
            pub fn from_slice(slice: &[u8]) -> Option<Self> {
                Some(Self(<$wrapper>::from_slice(slice)?))
            }
        }

        impl Deref for $name {
            type Target = [u8];

            #[inline]
            fn deref(&self) -> &[u8] {
                self.0.as_ref()
            }
        }

        impl AsRef<[u8]> for $name {
            #[inline]
            fn as_ref(&self) -> &[u8] {
                self.0.as_ref()
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut Formatter) -> fmt::Result {
                let digest = faster_hex::hex_string(self.as_ref()).unwrap();
                f.debug_tuple(stringify!($name)).field(&digest).finish()
            }
        }
    };
}

impl_wrapper!(Digest, sha256::Digest);
impl_wrapper!(Signature, sign::Signature);

#[inline]
pub fn double_sha256(buf: &[u8]) -> Digest {
    Digest(sha256::hash(sha256::hash(buf).as_ref()))
}

pub struct DoubleSha256(sha256::State);

impl DoubleSha256 {
    #[inline]
    pub fn new() -> Self {
        DoubleSha256(sha256::State::new())
    }

    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    #[inline]
    pub fn finalize(self) -> Digest {
        let digest = self.0.finalize();
        let digest = sha256::hash(digest.as_ref());
        Digest(digest)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ensure_double_hash() {
        let data = &[1, 2, 3, 4, 5];
        let digest_a = double_sha256(data);
        let digest_b = {
            let mut hasher = DoubleSha256::new();
            hasher.update(data);
            hasher.finalize()
        };
        assert_eq!(digest_a, digest_b);
    }
}
