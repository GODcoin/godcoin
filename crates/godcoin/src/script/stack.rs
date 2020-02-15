use super::{error::*, OpFrame, MAX_FRAME_STACK};
use crate::{
    asset::Asset,
    crypto::{PublicKey, ScriptHash},
};

#[derive(Debug)]
pub struct Stack {
    inner: Vec<OpFrame>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack {
            inner: Vec::with_capacity(MAX_FRAME_STACK),
        }
    }

    pub fn push<T>(&mut self, op: T) -> Result<(), EvalErrType>
    where
        T: Into<OpFrame>,
    {
        if self.inner.len() < MAX_FRAME_STACK {
            self.inner.push(op.into());
            Ok(())
        } else {
            Err(EvalErrType::StackOverflow)
        }
    }

    pub fn pop(&mut self) -> Result<OpFrame, EvalErrType> {
        self.inner.pop().ok_or(EvalErrType::StackUnderflow)
    }

    pub fn pop_bool(&mut self) -> Result<bool, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::False => Ok(false),
            OpFrame::True => Ok(true),
            _ => Err(EvalErrType::InvalidItemOnStack),
        }
    }

    pub fn pop_pubkey(&mut self) -> Result<PublicKey, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::PubKey(key) => Ok(key),
            _ => Err(EvalErrType::InvalidItemOnStack),
        }
    }

    pub fn pop_scripthash(&mut self) -> Result<ScriptHash, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::ScriptHash(hash) => Ok(hash),
            _ => Err(EvalErrType::InvalidItemOnStack),
        }
    }

    pub fn pop_asset(&mut self) -> Result<Asset, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::Asset(asset) => Ok(asset),
            _ => Err(EvalErrType::InvalidItemOnStack),
        }
    }

    #[inline]
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
