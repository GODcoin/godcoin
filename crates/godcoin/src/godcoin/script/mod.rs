use std::borrow::Cow;
use std::ops::Deref;

pub mod builder;
pub mod constants;
pub mod engine;
pub mod error;
pub mod op;
mod stack;

pub use self::builder::*;
pub use self::constants::*;
pub use self::engine::*;
pub use self::error::*;
pub use self::op::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Script(Vec<u8>);

impl Script {
    #[inline]
    pub fn new(byte_code: Vec<u8>) -> Script {
        Script(byte_code)
    }
}

impl From<&[u8]> for Script {
    #[inline]
    fn from(slice: &[u8]) -> Self {
        Script::new(slice.to_owned())
    }
}

impl From<Builder> for Script {
    fn from(b: Builder) -> Script {
        b.build()
    }
}

impl<'a> Into<Cow<'a, Script>> for Script {
    fn into(self) -> Cow<'a, Script> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, Script>> for &'a Script {
    fn into(self) -> Cow<'a, Script> {
        Cow::Borrowed(self)
    }
}

impl Deref for Script {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
