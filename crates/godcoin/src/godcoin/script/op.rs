use crate::crypto::PublicKey;

// TODO assign constants to ops
#[derive(PartialEq)]
#[repr(u8)]
pub enum Operand {
    // Push value
    PushFalse = 0x0,
    PushTrue = 0x1,
    PushPubKey,

    // Control
    OpIf,
    OpElse,
    OpEndIf,
    OpReturn
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
    OpReturn
}

impl From<Operand> for u8 {
    fn from(op: Operand) -> u8 {
        op as u8
    }
}
