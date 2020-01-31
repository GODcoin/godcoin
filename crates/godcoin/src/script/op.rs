use crate::{
    asset::Asset,
    crypto::{PublicKey, ScriptHash},
};

#[derive(PartialEq)]
#[repr(u8)]
pub enum Operand {
    // Push value
    PushFalse = 0x00,
    PushTrue = 0x01,
    PushPubKey = 0x02,
    PushScriptHash = 0x03,
    PushAsset = 0x04,

    // Arithmetic
    OpLoadAmt = 0x10,
    OpLoadRemAmt = 0x11,
    OpAdd = 0x12,
    OpSub = 0x13,
    OpMul = 0x14,
    OpDiv = 0x15,

    // Logic
    OpNot = 0x20,
    OpIf = 0x21,
    OpElse = 0x22,
    OpEndIf = 0x23,
    OpReturn = 0x24,

    // Crypto
    OpCheckSig = 0x30,
    OpCheckSigFastFail = 0x31,
    OpCheckMultiSig = 0x32,
    OpCheckMultiSigFastFail = 0x33,
}

impl From<Operand> for u8 {
    fn from(op: Operand) -> u8 {
        op as u8
    }
}

#[derive(Debug, PartialEq)]
pub enum OpFrame {
    // Push value
    False,
    True,
    PubKey(PublicKey),
    ScriptHash(ScriptHash),
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

    // Crypto
    OpCheckSig,
    OpCheckSigFastFail,
    OpCheckMultiSig(u8, u8), // M of N: minimum threshold to number of keys
    OpCheckMultiSigFastFail(u8, u8),
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
