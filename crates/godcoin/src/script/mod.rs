use crate::crypto::PublicKey;
use std::{
    borrow::Cow,
    fmt::{self, Debug, Formatter},
    ops::Deref,
};

pub mod builder;
pub mod engine;
pub mod error;
pub mod op;
mod stack;

pub use self::builder::*;
pub use self::engine::*;
pub use self::error::*;
pub use self::op::*;

pub const MAX_FRAME_STACK: usize = 64;

#[derive(Clone, PartialEq)]
pub struct Script(Vec<u8>);

impl Script {
    #[inline]
    pub fn new(byte_code: Vec<u8>) -> Self {
        Script(byte_code)
    }
}

impl Debug for Script {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let digest = faster_hex::hex_string(self.as_ref()).unwrap();
        f.debug_tuple("Script").field(&digest).finish()
    }
}

impl From<&[u8]> for Script {
    #[inline]
    fn from(slice: &[u8]) -> Self {
        Script::new(slice.to_owned())
    }
}

impl From<Vec<u8>> for Script {
    #[inline]
    fn from(vec: Vec<u8>) -> Self {
        Script::new(vec)
    }
}

impl From<Builder> for Script {
    #[inline]
    fn from(b: Builder) -> Self {
        b.build()
    }
}

impl From<PublicKey> for Script {
    fn from(key: PublicKey) -> Self {
        let builder = Builder::new()
            .push(OpFrame::PubKey(key))
            .push(OpFrame::OpCheckSig);
        builder.build()
    }
}

impl<'a> Into<Cow<'a, Script>> for Script {
    #[inline]
    fn into(self) -> Cow<'a, Script> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, Script>> for &'a Script {
    #[inline]
    fn into(self) -> Cow<'a, Script> {
        Cow::Borrowed(self)
    }
}

impl AsRef<[u8]> for Script {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for Script {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
