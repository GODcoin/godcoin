use super::{error::*, OpFrame, MAX_FRAME_STACK};
use crate::{account::AccountId, asset::Asset};

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

    pub fn push<T>(&mut self, op: T) -> Result<(), EvalErrKind>
    where
        T: Into<OpFrame>,
    {
        if self.inner.len() < MAX_FRAME_STACK {
            self.inner.push(op.into());
            Ok(())
        } else {
            Err(EvalErrKind::StackOverflow)
        }
    }

    pub fn pop(&mut self) -> Result<OpFrame, EvalErrKind> {
        self.inner.pop().ok_or(EvalErrKind::StackUnderflow)
    }

    pub fn pop_bool(&mut self) -> Result<bool, EvalErrKind> {
        let frame = self.pop()?;
        match frame {
            OpFrame::False => Ok(false),
            OpFrame::True => Ok(true),
            _ => Err(EvalErrKind::InvalidItemOnStack),
        }
    }

    pub fn pop_account_id(&mut self) -> Result<AccountId, EvalErrKind> {
        let frame = self.pop()?;
        match frame {
            OpFrame::AccountId(id) => Ok(id),
            _ => Err(EvalErrKind::InvalidItemOnStack),
        }
    }

    pub fn pop_asset(&mut self) -> Result<Asset, EvalErrKind> {
        let frame = self.pop()?;
        match frame {
            OpFrame::Asset(asset) => Ok(asset),
            _ => Err(EvalErrKind::InvalidItemOnStack),
        }
    }

    #[inline]
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
