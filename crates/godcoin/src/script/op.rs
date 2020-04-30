use crate::{account::AccountId, asset::Asset};
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Operand {
    // Function definition
    OpDefine = 0x00,

    // Events
    OpTransfer = 0x10,
    OpDestroy = 0x11,

    // Push value
    PushFalse = 0x20,
    PushTrue = 0x21,
    PushAccountId = 0x22,
    PushAsset = 0x23,

    // Arithmetic
    OpLoadAmt = 0x30,
    OpLoadRemAmt = 0x31,
    OpAdd = 0x32,
    OpSub = 0x33,
    OpMul = 0x34,
    OpDiv = 0x35,

    // Logic
    OpNot = 0x40,
    OpIf = 0x41,
    OpElse = 0x42,
    OpEndIf = 0x43,
    OpReturn = 0x44,
    OpAbort = 0x45,

    // Crypto
    OpCheckPerms = 0x50,
    OpCheckPermsFastFail = 0x51,
    OpCheckMultiPerms = 0x52,
    OpCheckMultiPermsFastFail = 0x53,

    // Lock time
    OpCheckTime = 0x60,
    OpCheckTimeFastFail = 0x61,
}

impl From<Operand> for u8 {
    fn from(op: Operand) -> u8 {
        op as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpFrame {
    // Function definition
    OpDefine(Vec<Arg>),

    // Events
    OpTransfer,
    OpDestroy,

    // Push value
    False,
    True,
    AccountId(AccountId),
    Asset(Asset),

    // Arithmetic
    OpLoadAmt,
    OpLoadRemAmt, // Load remaining amount
    OpAdd,
    OpSub,
    OpMul,
    OpDiv,

    // Logic
    OpNot,
    OpIf,
    OpElse,
    OpEndIf,
    OpReturn,
    OpAbort,

    // Crypto
    OpCheckPerms,
    OpCheckPermsFastFail,
    OpCheckMultiPerms(u8, u8), // M of N: minimum threshold to number of accounts
    OpCheckMultiPermsFastFail(u8, u8),

    // Lock time
    OpCheckTime(u64), // Epoch time in seconds
    OpCheckTimeFastFail(u64),
}

impl From<bool> for OpFrame {
    fn from(b: bool) -> OpFrame {
        if b {
            OpFrame::True
        } else {
            OpFrame::False
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Arg {
    AccountId = 0x00,
    Asset = 0x01,
}

impl TryFrom<u8> for Arg {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            t if t == Self::AccountId as u8 => Self::AccountId,
            t if t == Self::Asset as u8 => Self::Asset,
            _ => return Err(()),
        })
    }
}

impl Into<u8> for Arg {
    #[inline]
    fn into(self) -> u8 {
        self as u8
    }
}
