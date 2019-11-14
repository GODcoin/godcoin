use crate::crypto::PublicKey;

#[derive(PartialEq)]
#[repr(u8)]
pub enum Operand {
    // Push value
    PushFalse = 0x00,
    PushTrue = 0x01,
    PushPubKey = 0x02,

    // Stack manipulation
    OpNot = 0x10,

    // Control
    OpIf = 0x20,
    OpElse = 0x21,
    OpEndIf = 0x22,
    OpReturn = 0x23,

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

    // Stack manipulation
    OpNot,

    // Control
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
