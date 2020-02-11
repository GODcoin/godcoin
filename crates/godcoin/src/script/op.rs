use crate::{
    asset::Asset,
    crypto::{PublicKey, ScriptHash},
};

#[derive(PartialEq)]
#[repr(u8)]
pub enum Operand {
    // Function definition
    OpDefine = 0x00,

    // Push value
    PushFalse = 0x20,
    PushTrue = 0x21,
    PushPubKey = 0x22,
    PushScriptHash = 0x23,
    PushAsset = 0x24,

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

    // Crypto
    OpCheckSig = 0x50,
    OpCheckSigFastFail = 0x51,
    OpCheckMultiSig = 0x52,
    OpCheckMultiSigFastFail = 0x53,
}

impl From<Operand> for u8 {
    fn from(op: Operand) -> u8 {
        op as u8
    }
}

#[derive(Debug, PartialEq)]
pub enum OpFrame {
    // Function definition
    OpDefine,

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
