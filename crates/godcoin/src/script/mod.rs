use crate::{crypto::PublicKey, serializer::*};
use std::{
    borrow::Cow,
    fmt::{self, Debug, Formatter},
    io::{self, BufRead as IoBufRead, Cursor},
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

#[derive(Clone, PartialEq, Eq)]
pub struct Script(Vec<u8>);

impl Script {
    #[inline]
    pub fn new(byte_code: Vec<u8>) -> Self {
        Script(byte_code)
    }

    pub fn get_fn_ptr(&self, fn_id: u8) -> io::Result<Option<u32>> {
        let mut cur = Cursor::<&[u8]>::new(&self.0);
        let fn_count = cur.take_u8()?;
        for _ in 0..fn_count {
            let header_id = cur.take_u8()?;
            if header_id == fn_id {
                let pos = cur.take_u32()?;
                return Ok(Some(pos));
            }
            cur.consume(4);
        }

        Ok(None)
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

// TODO Create a default script for accounts
impl From<PublicKey> for Script {
    fn from(key: PublicKey) -> Self {
        let builder = Builder::new().push(
            FnBuilder::new(0x00, OpFrame::OpDefine(vec![Arg::AccountId, Arg::Asset]))
                .push(OpFrame::PubKey(key))
                .push(OpFrame::OpCheckSigFastFail)
                .push(OpFrame::OpTransfer)
                .push(OpFrame::True),
        );
        builder
            .build()
            .expect("Failed to build default script for PublicKey")
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
