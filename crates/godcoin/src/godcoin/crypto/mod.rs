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

macro_rules! impl_wrapper {
    ($name:ident, $wrapper:ty) => {
        #[derive(Clone, PartialEq, Eq, Hash)]
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
