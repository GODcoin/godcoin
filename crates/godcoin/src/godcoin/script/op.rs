use crate::crypto::PublicKey;

#[derive(PartialEq)]
#[repr(u8)]
pub enum Operand {
    // Push value
    PushFalse = 0x0,
    PushTrue = 0x1,
    PushPubKey = 0x2,

    // Control
    OpIf = 0x10,
    OpElse = 0x11,
    OpEndIf = 0x12,
    OpReturn = 0x13,

    // Crypto
    OpCheckSig = 0x20
}

#[derive(PartialEq)]
pub enum OpFrame {
    // Push value
    False,
    True,
    PubKey(PublicKey),

    // Control
    OpIf,
    OpElse,
    OpEndIf,
    OpReturn,

    // Crypto
    OpCheckSig
}

impl From<Operand> for u8 {
    fn from(op: Operand) -> u8 {
        op as u8
    }
}
