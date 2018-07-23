#[macro_use]
mod util;

pub mod tx;
pub use self::tx::*;

pub mod tx_type;
pub use self::tx_type::*;

use ::std::io::Cursor;

pub trait EncodeTx {
    fn encode(&self, v: &mut Vec<u8>);
}

pub trait DecodeTx<T> {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<T>;
}
