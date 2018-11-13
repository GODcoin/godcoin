use super::ScriptEngine;
use super::constants;
use super::op::*;

pub struct Builder {
    byte_code: Vec<u8>
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            byte_code: Vec::with_capacity(constants::MAX_BYTE_SIZE)
        }
    }

    pub fn build(self) -> Vec<u8> {
        self.byte_code
    }

    pub fn push(self, frame: OpFrame) -> Self {
        self.try_push(frame).unwrap()
    }

    pub fn try_push(self, frame: OpFrame) -> Option<Self> {
        match frame {
            OpFrame::False => self.insert_bytes(&[Operand::PushFalse.into()]),
            OpFrame::True => self.insert_bytes(&[Operand::PushTrue.into()]),
            OpFrame::PubKey(key) => {
                self.insert_bytes(&[Operand::PushPubKey.into()])?
                    .insert_bytes(key.as_bytes())
            },
            OpFrame::OpIf => self.insert_bytes(&[Operand::OpIf.into()]),
            OpFrame::OpElse => self.insert_bytes(&[Operand::OpElse.into()]),
            OpFrame::OpEndIf => self.insert_bytes(&[Operand::OpEndIf.into()]),
            OpFrame::OpReturn => self.insert_bytes(&[Operand::OpReturn.into()])
        }
    }

    fn insert_bytes(mut self, bytes: &[u8]) -> Option<Self> {
        if self.byte_code.len() + bytes.len() <= constants::MAX_BYTE_SIZE {
            self.byte_code.extend(bytes);
            Some(self)
        } else {
            None
        }
    }
}

impl From<Builder> for Vec<u8> {
    fn from(b: Builder) -> Vec<u8> {
        b.build()
    }
}

impl<'a> From<Builder> for ScriptEngine {
    fn from(b: Builder) -> Self {
        ScriptEngine::new(b.build()).unwrap()
    }
}
