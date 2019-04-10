use sodiumoxide::crypto::sign;
use std::borrow::Cow;

use super::stack::*;
use super::*;
use crate::crypto::PublicKey;
use crate::tx::TxVariant;

macro_rules! map_err_type {
    ($self:expr, $var:expr) => {
        $var.map_err(|e| $self.new_err(e))
    };
}

pub struct ScriptEngine<'a> {
    script: Cow<'a, Script>,
    tx: Cow<'a, TxVariant>,
    pos: usize,
    stack: Stack,
}

impl<'a> ScriptEngine<'a> {
    pub fn checked_new<T, S>(tx: T, script: S) -> Option<Self>
    where
        T: Into<Cow<'a, TxVariant>>,
        S: Into<Cow<'a, Script>>,
    {
        let script = script.into();
        let tx = tx.into();
        if script.len() > MAX_BYTE_SIZE {
            return None;
        }
        Some(Self {
            script,
            tx,
            pos: 0,
            stack: Stack::new(),
        })
    }

    pub fn eval(&mut self) -> Result<bool, EvalErr> {
        self.pos = 0;
        let mut if_marker = 0;
        let mut ignore_else = false;
        while let Some(op) = self.consume_op()? {
            match op {
                // Control
                OpFrame::OpIf => {
                    if_marker += 1;
                    ignore_else = map_err_type!(self, self.stack.pop_bool())?;
                    if ignore_else {
                        continue;
                    }
                    let req_if_marker = if_marker;
                    self.consume_op_until(|op| {
                        if op == OpFrame::OpIf {
                            if_marker += 1;
                            false
                        } else if op == OpFrame::OpElse {
                            if_marker == req_if_marker
                        } else if op == OpFrame::OpEndIf {
                            let do_break = if_marker == req_if_marker;
                            if_marker -= 1;
                            do_break
                        } else {
                            false
                        }
                    })?;
                }
                OpFrame::OpElse => {
                    if !ignore_else {
                        continue;
                    }
                    let req_if_marker = if_marker;
                    self.consume_op_until(|op| {
                        if op == OpFrame::OpIf {
                            if_marker += 1;
                            false
                        } else if op == OpFrame::OpElse {
                            if_marker == req_if_marker
                        } else if op == OpFrame::OpEndIf {
                            let do_break = if_marker == req_if_marker;
                            if_marker -= 1;
                            do_break
                        } else {
                            false
                        }
                    })?;
                }
                OpFrame::OpEndIf => {
                    if_marker -= 1;
                }
                OpFrame::OpReturn => {
                    if_marker = 0;
                    break;
                }
                // Crypto
                OpFrame::OpCheckSig => {
                    let key = map_err_type!(self, self.stack.pop_pubkey())?;
                    if self.tx.signature_pairs.len() != 1 {
                        map_err_type!(self, self.stack.push(false))?;
                        continue;
                    }
                    let mut buf = Vec::with_capacity(4096);
                    self.tx.encode(&mut buf);
                    let success = key.verify(&buf, &self.tx.signature_pairs[0].signature);
                    map_err_type!(self, self.stack.push(success))?;
                }
                // Handle push ops
                _ => {
                    map_err_type!(self, self.stack.push(op))?;
                }
            }
        }

        if if_marker > 0 {
            return Err(self.new_err(EvalErrType::UnexpectedEOF));
        }

        // Scripts must return true or false
        map_err_type!(self, self.stack.pop_bool())
    }

    fn consume_op_until<F>(&mut self, mut filter: F) -> Result<(), EvalErr>
    where
        F: FnMut(OpFrame) -> bool,
    {
        loop {
            match self.consume_op()? {
                Some(op) => {
                    if filter(op) {
                        break;
                    }
                }
                None => return Err(self.new_err(EvalErrType::UnexpectedEOF)),
            }
        }

        Ok(())
    }

    fn consume_op(&mut self) -> Result<Option<OpFrame>, EvalErr> {
        if self.pos == self.script.len() {
            return Ok(None);
        }
        let byte = self.script[self.pos];
        self.pos += 1;

        match byte {
            o if o == Operand::PushFalse as u8 => Ok(Some(OpFrame::False)),
            o if o == Operand::PushTrue as u8 => Ok(Some(OpFrame::True)),
            o if o == Operand::PushPubKey as u8 => {
                let slice = self.pos..self.pos + sign::PUBLICKEYBYTES;
                let key = match self.script.get(slice) {
                    Some(slice) => {
                        self.pos += sign::PUBLICKEYBYTES;
                        PublicKey::from_slice(slice).unwrap()
                    }
                    None => {
                        return Err(self.new_err(EvalErrType::UnexpectedEOF));
                    }
                };
                Ok(Some(OpFrame::PubKey(key)))
            }
            o if o == Operand::OpIf as u8 => Ok(Some(OpFrame::OpIf)),
            o if o == Operand::OpElse as u8 => Ok(Some(OpFrame::OpElse)),
            o if o == Operand::OpEndIf as u8 => Ok(Some(OpFrame::OpEndIf)),
            o if o == Operand::OpReturn as u8 => Ok(Some(OpFrame::OpReturn)),
            o if o == Operand::OpCheckSig as u8 => Ok(Some(OpFrame::OpCheckSig)),
            _ => Err(self.new_err(EvalErrType::UnknownOp)),
        }
    }

    fn new_err(&self, err: EvalErrType) -> EvalErr {
        EvalErr::new(self.pos, err)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::asset::Asset;
    use crate::crypto::KeyPair;
    use crate::tx::{SignTx, TransferTx, Tx, TxType};

    #[test]
    fn true_only_script() {
        let mut engine = new_engine(Builder::new().push(OpFrame::True));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn false_only_script() {
        let mut engine = new_engine(Builder::new().push(OpFrame::False));
        assert!(!engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn if_script() {
        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::True)
                .push(OpFrame::OpIf)
                    .push(OpFrame::False)
                .push(OpFrame::OpEndIf),
        );
        assert!(!engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::True)
                .push(OpFrame::OpIf)
                    .push(OpFrame::True)
                .push(OpFrame::OpEndIf),
        );
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn if_script_with_ret() {
        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::True)
                .push(OpFrame::OpIf)
                    .push(OpFrame::False)
                    .push(OpFrame::OpReturn)
                .push(OpFrame::OpEndIf)
                .push(OpFrame::True),
        );
        assert!(!engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::False)
                .push(OpFrame::OpIf)
                    .push(OpFrame::False)
                .push(OpFrame::OpElse)
                    .push(OpFrame::True)
                    .push(OpFrame::OpReturn)
                .push(OpFrame::OpEndIf)
                .push(OpFrame::False),
        );
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn branch_if() {
        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::True)
                .push(OpFrame::OpIf)
                    .push(OpFrame::True)
                .push(OpFrame::OpElse)
                    .push(OpFrame::False)
                .push(OpFrame::OpEndIf),
        );
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::False)
                .push(OpFrame::OpIf)
                    .push(OpFrame::False)
                .push(OpFrame::OpElse)
                    .push(OpFrame::True)
                .push(OpFrame::OpEndIf),
        );
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn nested_branch_if() {
        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::True)
                .push(OpFrame::OpIf)
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::True)
                    .push(OpFrame::OpEndIf)
                .push(OpFrame::OpElse)
                    .push(OpFrame::False)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::False)
                    .push(OpFrame::OpEndIf)
                .push(OpFrame::OpEndIf),
        );
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::False)
                .push(OpFrame::OpIf)
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::False)
                    .push(OpFrame::OpEndIf)
                .push(OpFrame::OpElse)
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::True)
                    .push(OpFrame::OpEndIf)
                .push(OpFrame::OpEndIf),
        );
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn fail_invalid_stack_on_return() {
        let key = KeyPair::gen_keypair().0;
        let mut engine = new_engine(Builder::new().push(OpFrame::PubKey(key)));
        assert_eq!(
            engine.eval().unwrap_err().err,
            EvalErrType::InvalidItemOnStack
        );
    }

    #[test]
    fn fail_invalid_if_cmp() {
        let key = KeyPair::gen_keypair().0;
        let mut engine = new_engine(
            Builder::new()
                .push(OpFrame::PubKey(key))
                .push(OpFrame::OpIf),
        );
        assert_eq!(
            engine.eval().unwrap_err().err,
            EvalErrType::InvalidItemOnStack
        );
    }

    #[test]
    fn fail_unended_if() {
        let mut engine = new_engine(Builder::new().push(OpFrame::True).push(OpFrame::OpIf));
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::UnexpectedEOF);

        let mut engine = new_engine(Builder::new().push(OpFrame::False).push(OpFrame::OpIf));
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::UnexpectedEOF);
    }

    #[test]
    fn checksig() {
        let key = KeyPair::gen_keypair();
        let mut engine = new_engine_with_signer(
            key.clone(),
            Builder::new()
                .push(OpFrame::PubKey(key.0.clone()))
                .push(OpFrame::OpCheckSig),
        );
        assert!(engine.eval().unwrap());

        let other = KeyPair::gen_keypair();
        let mut engine = new_engine_with_signer(
            key.clone(),
            Builder::new()
                .push(OpFrame::PubKey(other.0.clone()))
                .push(OpFrame::OpCheckSig),
        );
        assert!(!engine.eval().unwrap());

        let mut engine = new_engine_with_signer(
            other,
            Builder::new()
                .push(OpFrame::PubKey(key.0))
                .push(OpFrame::OpCheckSig),
        );
        assert!(!engine.eval().unwrap());
    }

    fn new_engine<'a>(builder: Builder) -> ScriptEngine<'a> {
        let from = KeyPair::gen_keypair();
        new_engine_with_signer(from, builder)
    }

    fn new_engine_with_signer<'a>(key: KeyPair, b: Builder) -> ScriptEngine<'a> {
        let to = KeyPair::gen_keypair();
        let script = b.build();

        let mut tx = TransferTx {
            base: Tx {
                tx_type: TxType::TRANSFER,
                timestamp: 1500000000,
                fee: Asset::from_str("1 GOLD").unwrap(),
                signature_pairs: vec![],
            },
            from: key.clone().0.into(),
            to: to.clone().0.into(),
            amount: Asset::from_str("10 GOLD").unwrap(),
            script: script.clone(),
            memo: vec![],
        };
        tx.append_sign(&key);

        ScriptEngine::checked_new(TxVariant::TransferTx(tx), script).unwrap()
    }
}
