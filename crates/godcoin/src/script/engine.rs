use sodiumoxide::crypto::sign;
use std::borrow::Cow;

use super::{stack::*, *};
use crate::{crypto::PublicKey, tx::TxPrecompData};

macro_rules! map_err_type {
    ($self:expr, $var:expr) => {
        $var.map_err(|e| $self.new_err(e))
    };
}

#[derive(Debug)]
pub struct ScriptEngine<'a> {
    script: Cow<'a, Script>,
    data: Cow<'a, TxPrecompData<'a>>,
    pos: usize,
    stack: Stack,
    sig_pair_pos: usize,
}

impl<'a> ScriptEngine<'a> {
    pub fn new<T, S>(data: T, script: S) -> Self
    where
        T: Into<Cow<'a, TxPrecompData<'a>>>,
        S: Into<Cow<'a, Script>>,
    {
        let script = script.into();
        let data = data.into();
        Self {
            script,
            data,
            pos: 0,
            stack: Stack::new(),
            sig_pair_pos: 0,
        }
    }

    pub fn eval(&mut self) -> Result<bool, EvalErr> {
        macro_rules! pop_multisig_keys {
            ($self:expr, $key_count:expr) => {{
                let mut vec = Vec::with_capacity(usize::from($key_count));
                for _ in 0..$key_count {
                    vec.push(map_err_type!($self, $self.stack.pop_pubkey())?);
                }
                vec
            }};
        }

        self.pos = 0;
        let mut if_marker = 0;
        let mut ignore_else = false;
        while let Some(op) = self.consume_op()? {
            match op {
                // Stack manipulation
                OpFrame::OpNot => {
                    let b = map_err_type!(self, self.stack.pop_bool())?;
                    map_err_type!(self, self.stack.push(!b))?;
                }
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
                    let success = self.check_sigs(1, &[key]);
                    map_err_type!(self, self.stack.push(success))?;
                }
                OpFrame::OpCheckSigFastFail => {
                    let key = map_err_type!(self, self.stack.pop_pubkey())?;
                    if !self.check_sigs(1, &[key]) {
                        return Ok(false);
                    }
                }
                OpFrame::OpCheckMultiSig(threshold, key_count) => {
                    let keys = pop_multisig_keys!(self, key_count);
                    let success = self.check_sigs(usize::from(threshold), &keys);
                    map_err_type!(self, self.stack.push(success))?;
                }
                OpFrame::OpCheckMultiSigFastFail(threshold, key_count) => {
                    let keys = pop_multisig_keys!(self, key_count);
                    if !self.check_sigs(usize::from(threshold), &keys) {
                        return Ok(false);
                    }
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

    fn consume_op_until<F>(&mut self, mut matcher: F) -> Result<(), EvalErr>
    where
        F: FnMut(OpFrame) -> bool,
    {
        loop {
            match self.consume_op()? {
                Some(op) => {
                    if matcher(op) {
                        break;
                    }
                }
                None => return Err(self.new_err(EvalErrType::UnexpectedEOF)),
            }
        }

        Ok(())
    }

    fn consume_op(&mut self) -> Result<Option<OpFrame>, EvalErr> {
        macro_rules! read_bytes {
            ($self:expr, $len:expr) => {
                match $self.script.get($self.pos..$self.pos + $len) {
                    Some(b) => {
                        $self.pos += $len;
                        b
                    }
                    None => {
                        return Err($self.new_err(EvalErrType::UnexpectedEOF));
                    }
                }
            };
            ($self:expr) => {
                match $self.script.get($self.pos) {
                    Some(b) => {
                        $self.pos += 1;
                        *b
                    }
                    None => {
                        return Err($self.new_err(EvalErrType::UnexpectedEOF));
                    }
                }
            };
        }

        if self.pos == self.script.len() {
            return Ok(None);
        }
        let byte = self.script[self.pos];
        self.pos += 1;

        match byte {
            // Push value
            o if o == Operand::PushFalse as u8 => Ok(Some(OpFrame::False)),
            o if o == Operand::PushTrue as u8 => Ok(Some(OpFrame::True)),
            o if o == Operand::PushPubKey as u8 => {
                let slice = read_bytes!(self, sign::PUBLICKEYBYTES);
                let key = PublicKey::from_slice(slice).unwrap();
                Ok(Some(OpFrame::PubKey(key)))
            }
            // Stack manipulation
            o if o == Operand::OpNot as u8 => Ok(Some(OpFrame::OpNot)),
            // Control
            o if o == Operand::OpIf as u8 => Ok(Some(OpFrame::OpIf)),
            o if o == Operand::OpElse as u8 => Ok(Some(OpFrame::OpElse)),
            o if o == Operand::OpEndIf as u8 => Ok(Some(OpFrame::OpEndIf)),
            o if o == Operand::OpReturn as u8 => Ok(Some(OpFrame::OpReturn)),
            // Crypto
            o if o == Operand::OpCheckSig as u8 => Ok(Some(OpFrame::OpCheckSig)),
            o if o == Operand::OpCheckSigFastFail as u8 => Ok(Some(OpFrame::OpCheckSigFastFail)),
            o if o == Operand::OpCheckMultiSig as u8 => {
                let threshold = read_bytes!(self);
                let key_count = read_bytes!(self);
                Ok(Some(OpFrame::OpCheckMultiSig(threshold, key_count)))
            }
            o if o == Operand::OpCheckMultiSigFastFail as u8 => {
                let threshold = read_bytes!(self);
                let key_count = read_bytes!(self);
                Ok(Some(OpFrame::OpCheckMultiSigFastFail(threshold, key_count)))
            }
            _ => Err(self.new_err(EvalErrType::UnknownOp)),
        }
    }

    fn check_sigs(&mut self, threshold: usize, keys: &[PublicKey]) -> bool {
        if threshold == 0 {
            return true;
        } else if threshold > keys.len() || self.sig_pair_pos >= self.data.tx().sigs().len() {
            return false;
        }

        let buf = self.data.bytes_without_sigs();
        let tx = &self.data.tx();
        let sigs = tx.sigs();

        let mut valid_threshold = 0;
        let mut key_iter = keys.iter();
        'pair_loop: for pair in &sigs[self.sig_pair_pos..] {
            loop {
                match key_iter.next() {
                    Some(key) => {
                        if key == &pair.pub_key {
                            self.sig_pair_pos += 1;
                            if key.verify(buf, &pair.signature) {
                                valid_threshold += 1;
                                continue 'pair_loop;
                            } else {
                                return false;
                            }
                        }
                    }
                    None => break 'pair_loop,
                }
            }
        }

        valid_threshold >= threshold
    }

    fn new_err(&self, err: EvalErrType) -> EvalErr {
        EvalErr::new(self.pos as u32, err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyPair, SigPair, Signature};
    use crate::tx::{TransferTx, Tx, TxVariant, TxVariantV0};

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
        let key = KeyPair::gen().0;
        let mut engine = new_engine(Builder::new().push(OpFrame::PubKey(key)));
        assert_eq!(
            engine.eval().unwrap_err().err,
            EvalErrType::InvalidItemOnStack
        );
    }

    #[test]
    fn fail_invalid_if_cmp() {
        let key = KeyPair::gen().0;
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
    fn checksig_pubkey_into_script_converted() {
        let key = KeyPair::gen();
        let script: Script = key.0.clone().into();

        let mut engine = {
            let tx = new_transfer_tx(script.clone(), &[key]);
            ScriptEngine::new(tx.precompute(), script)
        };

        assert!(engine.eval().unwrap());
    }

    #[test]
    fn checksig() {
        let key = KeyPair::gen();
        let mut engine = new_engine_with_signers(
            &[key.clone()],
            Builder::new()
                .push(OpFrame::PubKey(key.0.clone()))
                .push(OpFrame::OpCheckSig),
        );
        assert!(engine.eval().unwrap());

        let other = KeyPair::gen();
        let mut engine = new_engine_with_signers(
            &[key.clone()],
            Builder::new()
                .push(OpFrame::PubKey(other.0.clone()))
                .push(OpFrame::OpCheckSig),
        );
        assert!(!engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[other],
            Builder::new()
                .push(OpFrame::PubKey(key.0))
                .push(OpFrame::OpCheckSig),
        );
        assert!(!engine.eval().unwrap());
    }

    #[test]
    fn checkmultisig_equal_threshold() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();

        let mut engine = new_engine_with_signers(
            &[key_3.clone(), key_1.clone()],
            Builder::new()
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSig(2, 3)),
        );
        assert!(engine.eval().unwrap());
    }

    #[test]
    fn checkmultisig_threshold_unmet() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();

        let mut engine = new_engine_with_signers(
            &[key_3.clone(), key_1.clone()],
            Builder::new()
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSig(3, 3)),
        );
        assert!(!engine.eval().unwrap());
    }

    #[test]
    fn checkmultisig_return_true() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        let builder = Builder::new()
            .push(OpFrame::PubKey(key_1.0.clone()))
            .push(OpFrame::PubKey(key_2.0.clone()))
            .push(OpFrame::PubKey(key_3.0.clone()))
            .push(OpFrame::OpCheckMultiSig(2, 3));

        let mut engine = new_engine_with_signers(
            &[key_2.clone(), key_1.clone(), KeyPair::gen()],
            builder.clone(),
        );
        // This should evaluate to true as the threshold is met, and the trailing signatures are
        // ignored by the script.
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(&[key_3.clone(), key_1.clone()], builder.clone());
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(&[key_2.clone(), key_1.clone()], builder.clone());
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[key_3.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());
    }

    #[test]
    fn checkmultisig_return_false() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        let builder = Builder::new()
            .push(OpFrame::PubKey(key_1.0.clone()))
            .push(OpFrame::PubKey(key_2.0.clone()))
            .push(OpFrame::PubKey(key_3.0.clone()))
            .push(OpFrame::OpCheckMultiSig(2, 3));

        let mut engine = new_engine_with_signers(
            &[KeyPair::gen(), key_3.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert!(!engine.eval().unwrap());

        let mut engine = {
            let to = KeyPair::gen();
            let script = builder.build();

            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    timestamp: 1500000000,
                    fee: "1.00000 GRAEL".parse().unwrap(),
                    signature_pairs: vec![SigPair {
                        // Test valid key with invalid signature
                        pub_key: key_3.0.clone(),
                        signature: Signature(sign::Signature([0; sign::SIGNATUREBYTES])),
                    }],
                },
                from: key_1.clone().0.into(),
                to: to.clone().0.into(),
                amount: "10.00000 GRAEL".parse().unwrap(),
                script: script.clone(),
                memo: vec![],
            }));
            tx.append_sign(&key_2);
            tx.append_sign(&key_1);

            ScriptEngine::new(tx.precompute(), script)
        };
        assert!(!engine.eval().unwrap());
    }

    #[test]
    fn checkmultisig_with_trailing_sig_fastfail() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        let key_4 = KeyPair::gen();
        #[rustfmt::skip]
        let builder = Builder::new()
            .push(OpFrame::PubKey(key_1.0.clone()))
            .push(OpFrame::PubKey(key_2.0.clone()))
            .push(OpFrame::PubKey(key_3.0.clone()))
            .push(OpFrame::PubKey(key_4.0.clone()))
            .push(OpFrame::OpCheckMultiSigFastFail(2, 4))
            .push(OpFrame::PubKey(key_0.0.clone()))
            .push(OpFrame::OpCheckSig);

        let mut engine = new_engine_with_signers(
            &[key_3.clone(), key_2.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[key_3.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[key_4.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[key_3.clone(), key_2.clone(), key_0.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[key_2.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        let mut engine = new_engine_with_signers(&[key_2.clone(), key_1.clone()], builder.clone());
        assert!(!engine.eval().unwrap());

        let mut engine = new_engine_with_signers(
            &[key_4.clone(), key_3.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert!(!engine.eval().unwrap());

        let mut engine = new_engine_with_signers(&[key_4.clone(), key_0.clone()], builder.clone());
        assert!(!engine.eval().unwrap());

        let mut engine = new_engine(builder.clone());
        assert!(!engine.eval().unwrap());
    }

    #[test]
    fn checkmultisig_with_trailing_sig_ignore_multisig_res() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        let key_4 = KeyPair::gen();
        #[rustfmt::skip]
        let builder = Builder::new()
            .push(OpFrame::PubKey(key_1.0.clone()))
            .push(OpFrame::PubKey(key_2.0.clone()))
            .push(OpFrame::PubKey(key_3.0.clone()))
            .push(OpFrame::PubKey(key_4.0.clone()))
            .push(OpFrame::OpCheckMultiSig(3, 4))
            .push(OpFrame::PubKey(key_0.0.clone()))
            .push(OpFrame::OpCheckSig);

        let mut engine = new_engine_with_signers(
            &[key_2.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = new_engine_with_signers(&[key_2.clone(), key_0.clone()], builder.clone());
        assert!(engine.eval().unwrap());
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = new_engine_with_signers(&[key_0.clone(), KeyPair::gen()], builder.clone());
        assert!(engine.eval().unwrap());
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = new_engine_with_signers(&[key_0.clone()], builder.clone());
        assert!(engine.eval().unwrap());
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn checksig_and_checkmultisig_with_if() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        #[rustfmt::skip]
        let builder = Builder::new()
            .push(OpFrame::PubKey(key_0.0.clone()))
            .push(OpFrame::OpCheckSig)
            .push(OpFrame::OpIf)
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSig(2, 3))
                .push(OpFrame::OpReturn)
            .push(OpFrame::OpEndIf)
            .push(OpFrame::False);

        // Test threshold is met and tx is signed with key_0
        let mut engine = new_engine_with_signers(
            &[key_0.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        // Test tx must be signed with key_0 but threshold is met
        let mut engine = new_engine_with_signers(&[key_1.clone(), key_2.clone()], builder.clone());
        assert!(!engine.eval().unwrap());

        // Test multisig threshold not met
        let mut engine = new_engine_with_signers(&[key_0.clone(), key_1.clone()], builder);
        assert!(!engine.eval().unwrap());
    }

    #[test]
    fn checksig_and_checkmultisig_with_if_not() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        #[rustfmt::skip]
        let builder = Builder::new()
            .push(OpFrame::PubKey(key_0.0.clone()))
            .push(OpFrame::OpCheckSig)
            .push(OpFrame::OpNot)
            .push(OpFrame::OpIf)
                .push(OpFrame::False)
                .push(OpFrame::OpReturn)
            .push(OpFrame::OpEndIf)
            .push(OpFrame::PubKey(key_1.0.clone()))
            .push(OpFrame::PubKey(key_2.0.clone()))
            .push(OpFrame::PubKey(key_3.0.clone()))
            .push(OpFrame::OpCheckMultiSig(2, 3));

        // Test threshold is met and tx is signed with key_0
        let mut engine = new_engine_with_signers(
            &[key_0.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert!(engine.eval().unwrap());

        // Test tx must be signed with key_0 but threshold is met
        let mut engine = new_engine_with_signers(&[key_1.clone(), key_2.clone()], builder.clone());
        assert!(!engine.eval().unwrap());

        // Test multisig threshold not met
        let mut engine = new_engine_with_signers(&[key_0.clone(), key_1.clone()], builder);
        assert!(!engine.eval().unwrap());
    }

    #[test]
    fn checksig_and_checkmultisig_with_fast_fail() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();

        // Test tx must be signed with key_0 but threshold is met
        #[rustfmt::skip]
        let mut engine = new_engine_with_signers(
            &[key_1.clone(), key_2.clone()],
            Builder::new()
                .push(OpFrame::PubKey(key_0.0.clone()))
                .push(OpFrame::OpCheckSigFastFail)
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSig(2, 3))
        );
        assert!(!engine.eval().unwrap());

        // Test multisig threshold not met
        #[rustfmt::skip]
        let mut engine = new_engine_with_signers(
            &[key_0.clone(), key_1.clone()],
            Builder::new()
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSigFastFail(2, 3))
                .push(OpFrame::PubKey(key_0.0.clone()))
                .push(OpFrame::OpCheckSig)
        );
        assert!(!engine.eval().unwrap());
    }

    fn new_engine<'a>(builder: Builder) -> ScriptEngine<'a> {
        let from = KeyPair::gen();
        new_engine_with_signers(&[from], builder)
    }

    fn new_engine_with_signers<'a>(keys: &[KeyPair], b: Builder) -> ScriptEngine<'a> {
        let script = b.build();
        let tx = new_transfer_tx(script.clone(), keys);
        ScriptEngine::new(tx.precompute(), script)
    }

    fn new_transfer_tx(script: Script, keys: &[KeyPair]) -> TxVariant {
        let to = KeyPair::gen();
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: Tx {
                timestamp: 1500000000,
                fee: "1.00000 GRAEL".parse().unwrap(),
                signature_pairs: vec![],
            },
            from: to.clone().0.into(),
            to: to.clone().0.into(),
            amount: "10.00000 GRAEL".parse().unwrap(),
            script: script.clone(),
            memo: vec![],
        }));
        keys.iter().for_each(|key| tx.append_sign(key));
        tx
    }
}
