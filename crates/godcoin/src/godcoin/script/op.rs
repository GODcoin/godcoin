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
    OpCheckMultiSig = 0x31,
}

impl From<Operand> for u8 {
    fn from(op: Operand) -> u8 {
        op as u8
    }
}

#[derive(PartialEq, Debug)]
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
    OpCheckMultiSig(u8, u8), // M of N: minimum threshold to number of keys
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
