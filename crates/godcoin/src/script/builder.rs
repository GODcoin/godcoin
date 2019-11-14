use super::{op::*, Script};
use crate::constants::MAX_SCRIPT_BYTE_SIZE;

#[derive(Clone, Debug, Default)]
pub struct Builder {
    byte_code: Vec<u8>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            byte_code: Vec::with_capacity(MAX_SCRIPT_BYTE_SIZE),
        }
    }

    pub fn build(self) -> Script {
        self.byte_code.into()
    }

    pub fn push(self, frame: OpFrame) -> Self {
        self.try_push(frame).expect("script byte size exceeded")
    }

    pub fn try_push(mut self, frame: OpFrame) -> Option<Self> {
        match frame {
            // Push value
            OpFrame::False => self.insert_bytes(&[Operand::PushFalse.into()])?,
            OpFrame::True => self.insert_bytes(&[Operand::PushTrue.into()])?,
            OpFrame::PubKey(key) => {
                self.insert_bytes(&[Operand::PushPubKey.into()])?;
                self.insert_bytes(key.as_ref())?;
            }
            // Stack manipulation
            OpFrame::OpNot => self.insert_bytes(&[Operand::OpNot.into()])?,
            // Control
            OpFrame::OpIf => self.insert_bytes(&[Operand::OpIf.into()])?,
            OpFrame::OpElse => self.insert_bytes(&[Operand::OpElse.into()])?,
            OpFrame::OpEndIf => self.insert_bytes(&[Operand::OpEndIf.into()])?,
            OpFrame::OpReturn => self.insert_bytes(&[Operand::OpReturn.into()])?,
            // Crypto
            OpFrame::OpCheckSig => self.insert_bytes(&[Operand::OpCheckSig.into()])?,
            OpFrame::OpCheckSigFastFail => {
                self.insert_bytes(&[Operand::OpCheckSigFastFail.into()])?
            }
            OpFrame::OpCheckMultiSig(threshold, key_count) => {
                self.insert_bytes(&[Operand::OpCheckMultiSig.into(), threshold, key_count])?
            }
            OpFrame::OpCheckMultiSigFastFail(threshold, key_count) => self.insert_bytes(&[
                Operand::OpCheckMultiSigFastFail.into(),
                threshold,
                key_count,
            ])?,
        }
        Some(self)
    }

    #[must_use]
    fn insert_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        if self.byte_code.len() + bytes.len() <= MAX_SCRIPT_BYTE_SIZE {
            self.byte_code.extend(bytes);
            Some(())
        } else {
            None
        }
    }
}

impl AsRef<[u8]> for Builder {
    fn as_ref(&self) -> &[u8] {
        &self.byte_code
    }
}
