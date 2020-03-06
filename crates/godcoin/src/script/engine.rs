use sodiumoxide::crypto::sign;
use std::{borrow::Cow, convert::TryInto, mem};

use super::{stack::*, *};
use crate::{
    asset::Asset,
    blockchain::LogEntry,
    crypto::{PublicKey, ScriptHash, SCRIPT_HASH_BYTES},
    serializer::BufRead,
    tx::{TxPrecompData, TxVariant, TxVariantV0},
};

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
    log: Vec<LogEntry>,
    total_amt: Asset,
    remaining_amt: Asset,
}

impl<'a> ScriptEngine<'a> {
    pub fn new<T, S>(data: T, script: S) -> Self
    where
        T: Into<Cow<'a, TxPrecompData<'a>>>,
        S: Into<Cow<'a, Script>>,
    {
        let script = script.into();
        let data = data.into();

        let total_amt = match data.tx() {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::TransferTx(tx) => tx.amount,
                _ => Asset::default(),
            },
        };

        Self {
            script,
            data,
            pos: 0,
            stack: Stack::new(),
            sig_pair_pos: 0,
            log: vec![],
            total_amt,
            remaining_amt: total_amt,
        }
    }

    /// Returns the log the script produces after execution completes. If any error occurs during evaluation, execution
    /// will be aborted and return an error.
    #[inline]
    pub fn eval(mut self) -> Result<Vec<LogEntry>, EvalErr> {
        let fn_id = match self.data.tx() {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::OwnerTx(_) => 0,
                TxVariantV0::MintTx(_) => 0,
                TxVariantV0::CreateAccountTx(_) => 0,
                TxVariantV0::TransferTx(tx) => tx.call_fn,
            },
        };
        self.call_fn(fn_id)
    }

    fn call_fn(&mut self, fn_id: u8) -> Result<Vec<LogEntry>, EvalErr> {
        macro_rules! pop_multisig_keys {
            ($self:expr, $key_count:expr) => {{
                let mut vec = Vec::with_capacity(usize::from($key_count));
                for _ in 0..$key_count {
                    vec.push(map_err_type!($self, $self.stack.pop_pubkey())?);
                }
                vec
            }};
        }

        self.pos = self
            .script
            .get_fn_ptr(fn_id)
            .map_err(|_| self.new_err(EvalErrType::HeaderReadErr))?
            .ok_or_else(|| self.new_err(EvalErrType::UnknownFn))? as usize;

        {
            let op = self.consume_op()?;
            match op {
                Some(OpFrame::OpDefine(args)) => {
                    let mut bin_args = Cursor::<&[u8]>::new(match self.data.tx() {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(_) => &[],
                            TxVariantV0::MintTx(_) => &[],
                            TxVariantV0::CreateAccountTx(_) => &[],
                            TxVariantV0::TransferTx(tx) => &tx.args,
                        },
                    });
                    for arg in args {
                        match arg {
                            Arg::ScriptHash => {
                                let digest = bin_args
                                    .take_digest()
                                    .map_err(|_| self.new_err(EvalErrType::ArgDeserialization))?;
                                let hash = ScriptHash(digest);
                                map_err_type!(self, self.stack.push(OpFrame::ScriptHash(hash)))?;
                            }
                            Arg::Asset => {
                                let asset = bin_args
                                    .take_asset()
                                    .map_err(|_| self.new_err(EvalErrType::ArgDeserialization))?;
                                map_err_type!(self, self.stack.push(OpFrame::Asset(asset)))?;
                            }
                        }
                    }
                }
                _ => return Err(self.new_err(EvalErrType::InvalidEntryPoint)),
            }
        }

        let mut if_marker = 0;
        let mut ignore_else = false;
        while let Some(op) = self.consume_op()? {
            match op {
                // Function definition
                OpFrame::OpDefine(_) => {
                    // We reached the next function definition, this function has no more ops to execute
                    break;
                }
                // Events
                OpFrame::OpTransfer => {
                    let amt = map_err_type!(self, self.stack.pop_asset())?;
                    let transfer_to = map_err_type!(self, self.stack.pop_scripthash())?;
                    if amt.amount < 0 || amt > self.remaining_amt {
                        return Err(self.new_err(EvalErrType::InvalidAmount));
                    }
                    self.remaining_amt = self
                        .remaining_amt
                        .checked_sub(amt)
                        .ok_or_else(|| self.new_err(EvalErrType::Arithmetic))?;
                    self.log.push(LogEntry::Transfer(transfer_to, amt));
                }
                // Push
                OpFrame::False => map_err_type!(self, self.stack.push(op))?,
                OpFrame::True => map_err_type!(self, self.stack.push(op))?,
                OpFrame::PubKey(_) => map_err_type!(self, self.stack.push(op))?,
                OpFrame::ScriptHash(_) => map_err_type!(self, self.stack.push(op))?,
                OpFrame::Asset(_) => map_err_type!(self, self.stack.push(op))?,
                // Arithmetic
                OpFrame::OpLoadAmt => {
                    map_err_type!(self, self.stack.push(OpFrame::Asset(self.total_amt)))?;
                }
                OpFrame::OpLoadRemAmt => {
                    map_err_type!(self, self.stack.push(OpFrame::Asset(self.remaining_amt)))?;
                }
                OpFrame::OpAdd => {
                    let b = map_err_type!(self, self.stack.pop_asset())?;
                    let a = map_err_type!(self, self.stack.pop_asset())?;
                    let res = a
                        .checked_add(b)
                        .ok_or_else(|| self.new_err(EvalErrType::Arithmetic))?;
                    map_err_type!(self, self.stack.push(OpFrame::Asset(res)))?;
                }
                OpFrame::OpSub => {
                    let b = map_err_type!(self, self.stack.pop_asset())?;
                    let a = map_err_type!(self, self.stack.pop_asset())?;
                    let res = a
                        .checked_sub(b)
                        .ok_or_else(|| self.new_err(EvalErrType::Arithmetic))?;
                    map_err_type!(self, self.stack.push(OpFrame::Asset(res)))?;
                }
                OpFrame::OpMul => {
                    let b = map_err_type!(self, self.stack.pop_asset())?;
                    let a = map_err_type!(self, self.stack.pop_asset())?;
                    let res = a
                        .checked_mul(b)
                        .ok_or_else(|| self.new_err(EvalErrType::Arithmetic))?;
                    map_err_type!(self, self.stack.push(OpFrame::Asset(res)))?;
                }
                OpFrame::OpDiv => {
                    let b = map_err_type!(self, self.stack.pop_asset())?;
                    let a = map_err_type!(self, self.stack.pop_asset())?;
                    let res = a
                        .checked_div(b)
                        .ok_or_else(|| self.new_err(EvalErrType::Arithmetic))?;
                    map_err_type!(self, self.stack.push(OpFrame::Asset(res)))?;
                }
                // Logic
                OpFrame::OpNot => {
                    let b = map_err_type!(self, self.stack.pop_bool())?;
                    map_err_type!(self, self.stack.push(!b))?;
                }
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
                        return Err(self.new_err(EvalErrType::ScriptRetFalse));
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
                        return Err(self.new_err(EvalErrType::ScriptRetFalse));
                    }
                }
            }
        }

        if if_marker > 0 {
            return Err(self.new_err(EvalErrType::UnexpectedEOF));
        }

        // Scripts must return true or false
        if map_err_type!(self, self.stack.pop_bool())? {
            let mut log = vec![];
            mem::swap(&mut self.log, &mut log);
            if self.remaining_amt.amount > 0 {
                // Send back funds to the original sender
                match self.data.tx() {
                    TxVariant::V0(tx) => match tx {
                        TxVariantV0::TransferTx(tx) => {
                            log.push(LogEntry::Transfer(tx.from.clone(), self.remaining_amt))
                        }
                        _ => return Err(self.new_err(EvalErrType::InvalidAmount)),
                    },
                }
                self.remaining_amt = Asset::default();
            }
            Ok(log)
        } else {
            Err(self.new_err(EvalErrType::ScriptRetFalse))
        }
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
            // Function definition
            o if o == Operand::OpDefine as u8 => {
                let arg_cnt = read_bytes!(self);
                let mut args = Vec::with_capacity(usize::from(arg_cnt));
                for _ in 0..arg_cnt {
                    let tag_byte = read_bytes!(self);
                    let arg = tag_byte
                        .try_into()
                        .map_err(|_| self.new_err(EvalErrType::UnknownArgType))?;
                    args.push(arg);
                }
                Ok(Some(OpFrame::OpDefine(args)))
            }
            // Events
            o if o == Operand::OpTransfer as u8 => Ok(Some(OpFrame::OpTransfer)),
            // Push value
            o if o == Operand::PushFalse as u8 => Ok(Some(OpFrame::False)),
            o if o == Operand::PushTrue as u8 => Ok(Some(OpFrame::True)),
            o if o == Operand::PushPubKey as u8 => {
                let slice = read_bytes!(self, sign::PUBLICKEYBYTES);
                let key = PublicKey::from_slice(slice).unwrap();
                Ok(Some(OpFrame::PubKey(key)))
            }
            o if o == Operand::PushScriptHash as u8 => {
                let slice = read_bytes!(self, SCRIPT_HASH_BYTES);
                let hash = ScriptHash::from_slice(slice).unwrap();
                Ok(Some(OpFrame::ScriptHash(hash)))
            }
            o if o == Operand::PushAsset as u8 => {
                let slice = read_bytes!(self, mem::size_of::<i64>());
                let amt = i64::from_be_bytes(slice.try_into().unwrap());
                let amt = Asset::new(amt);
                Ok(Some(OpFrame::Asset(amt)))
            }
            // Arithmetic
            o if o == Operand::OpLoadAmt as u8 => Ok(Some(OpFrame::OpLoadAmt)),
            o if o == Operand::OpLoadRemAmt as u8 => Ok(Some(OpFrame::OpLoadRemAmt)),
            o if o == Operand::OpAdd as u8 => Ok(Some(OpFrame::OpAdd)),
            o if o == Operand::OpSub as u8 => Ok(Some(OpFrame::OpSub)),
            o if o == Operand::OpMul as u8 => Ok(Some(OpFrame::OpMul)),
            o if o == Operand::OpDiv as u8 => Ok(Some(OpFrame::OpDiv)),
            // Logic
            o if o == Operand::OpNot as u8 => Ok(Some(OpFrame::OpNot)),
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

        let txid = self.data.txid().as_ref();
        let sigs = self.data.tx().sigs();

        let mut valid_threshold = 0;
        let mut key_iter = keys.iter();
        'pair_loop: for pair in &sigs[self.sig_pair_pos..] {
            loop {
                match key_iter.next() {
                    Some(key) => {
                        if key == &pair.pub_key {
                            self.sig_pair_pos += 1;
                            if key.verify(txid, &pair.signature) {
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
    use std::ops::DerefMut;

    #[test]
    fn true_only_script() {
        let mut engine = TestEngine::new().init(
            Builder::new().push(FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::True)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn false_only_script() {
        let mut engine = TestEngine::new().init(
            Builder::new().push(FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::False)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn push_asset() {
        let asset = "100.00000 TEST".parse().unwrap();
        let frame = OpFrame::Asset(asset);
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(frame)
                    .push(OpFrame::True),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(engine.stack.pop_asset().unwrap(), asset);
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn arithmetic_loadamt() {
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::OpLoadAmt)
                    .push(OpFrame::True),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.stack.pop_asset().unwrap(),
            "10.00000 TEST".parse().unwrap()
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn arithmetic_loadremamt() {
        let mut engine = {
            let engine = TestEngine::new();

            let script = Builder::new()
                .push(
                    FnBuilder::new(0, OpFrame::OpDefine(vec![Arg::ScriptHash]))
                        .push(OpFrame::OpLoadRemAmt)
                        .push(OpFrame::OpTransfer)
                        .push(OpFrame::OpLoadRemAmt)
                        .push(OpFrame::True),
                )
                .build()
                .unwrap();
            let mut args = vec![];
            args.push_scripthash(&engine.to_addr.clone().0.into());
            let tx = engine.new_transfer_tx(script.clone(), 0, args, &[]);

            engine.init_direct(tx, script)
        };
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.to_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.stack.pop_asset().unwrap(),
            "0.00000 TEST".parse().unwrap()
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn arithmetic_add() {
        let asset_a = "100.00000 TEST".parse().unwrap();
        let asset_b = "0.12345 TEST".parse().unwrap();
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::Asset(asset_a))
                    .push(OpFrame::Asset(asset_b))
                    .push(OpFrame::OpAdd)
                    .push(OpFrame::True),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.stack.pop_asset().unwrap(),
            "100.12345 TEST".parse().unwrap()
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn arithmetic_sub() {
        let asset_a = "100.00000 TEST".parse().unwrap();
        let asset_b = "1.00000 TEST".parse().unwrap();
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::Asset(asset_a))
                    .push(OpFrame::Asset(asset_b))
                    .push(OpFrame::OpSub)
                    .push(OpFrame::True),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.stack.pop_asset().unwrap(),
            "99.00000 TEST".parse().unwrap()
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn arithmetic_mul() {
        let asset_a = "50.00000 TEST".parse().unwrap();
        let asset_b = "1.50000 TEST".parse().unwrap();
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::Asset(asset_a))
                    .push(OpFrame::Asset(asset_b))
                    .push(OpFrame::OpMul)
                    .push(OpFrame::True),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.stack.pop_asset().unwrap(),
            "75.00000 TEST".parse().unwrap()
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn arithmetic_div() {
        let asset_a = "75.00000 TEST".parse().unwrap();
        let asset_b = "2.00000 TEST".parse().unwrap();
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::Asset(asset_a))
                    .push(OpFrame::Asset(asset_b))
                    .push(OpFrame::OpDiv)
                    .push(OpFrame::True),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.stack.pop_asset().unwrap(),
            "37.50000 TEST".parse().unwrap()
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn call_unknown_fn() {
        let mut engine = TestEngine::new().init(
            Builder::new().push(FnBuilder::new(1, OpFrame::OpDefine(vec![])).push(OpFrame::True)),
        );
        assert_eq!(engine.call_fn(0).unwrap_err().err, EvalErrType::UnknownFn);
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn call_different_fns() {
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::False))
                .push(FnBuilder::new(1, OpFrame::OpDefine(vec![])).push(OpFrame::True)),
        );
        assert_eq!(
            engine.call_fn(1).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn call_args_pushed_on_stack() {
        let script = Builder::new()
            .push(
                FnBuilder::new(1, OpFrame::OpDefine(vec![Arg::ScriptHash, Arg::Asset]))
                    .push(OpFrame::True),
            )
            .build()
            .unwrap();
        let hash = {
            let pub_key = KeyPair::gen().0;
            ScriptHash::from(Script::from(pub_key))
        };
        let asset = "1234.00000 TEST".parse().unwrap();

        let mut engine = {
            let mut args = vec![];
            args.push_scripthash(&hash);
            args.push_asset(asset);
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(script.clone(), 1, args, &[]);
            engine.init_direct(tx, script)
        };
        assert_eq!(
            engine.call_fn(1).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(engine.stack.pop_asset(), Ok(asset));
        assert_eq!(engine.stack.pop_scripthash(), Ok(hash));
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn eval_uses_transfer_tx_call_fn() {
        let script = Builder::new()
            .push(FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::False))
            .push(FnBuilder::new(1, OpFrame::OpDefine(vec![])).push(OpFrame::True))
            .build()
            .unwrap();

        {
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(script.clone(), 0, vec![], &[]);
            let engine = engine.init_direct(tx, script.clone());
            assert_eq!(engine.eval().unwrap_err().err, EvalErrType::ScriptRetFalse);
        }
        {
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(script.clone(), 1, vec![], &[]);
            let engine = engine.init_direct(tx, script.clone());
            let from_entry = engine.from_transfer_entry("10.00000 TEST");
            assert_eq!(engine.eval().unwrap(), vec![from_entry]);
        }
        {
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(script.clone(), 2, vec![], &[]);
            let engine = engine.init_direct(tx, script.clone());
            assert_eq!(engine.eval().unwrap_err().err, EvalErrType::UnknownFn);
        }
    }

    #[test]
    fn if_script() {
        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::False)
                    .push(OpFrame::OpEndIf)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::True)
                    .push(OpFrame::OpEndIf)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn if_script_with_ret() {
        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::False)
                        .push(OpFrame::OpReturn)
                    .push(OpFrame::OpEndIf)
                    .push(OpFrame::True)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
            .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::False)
                .push(OpFrame::OpIf)
                    .push(OpFrame::False)
                .push(OpFrame::OpElse)
                    .push(OpFrame::True)
                    .push(OpFrame::OpReturn)
                .push(OpFrame::OpEndIf)
                .push(OpFrame::False)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn branch_if() {
        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::True)
                    .push(OpFrame::OpElse)
                        .push(OpFrame::False)
                    .push(OpFrame::OpEndIf)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::False)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::False)
                    .push(OpFrame::OpElse)
                        .push(OpFrame::True)
                    .push(OpFrame::OpEndIf)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn nested_branch_if() {
        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
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
                    .push(OpFrame::OpEndIf)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());

        #[rustfmt::skip]
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
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
                    .push(OpFrame::OpEndIf)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn fail_invalid_stack_on_return() {
        let key = KeyPair::gen().0;
        let mut engine = TestEngine::new().init(
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::PubKey(key))),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::InvalidItemOnStack
        );
    }

    #[test]
    fn fail_invalid_if_cmp() {
        let key = KeyPair::gen().0;
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key))
                    .push(OpFrame::OpIf),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::InvalidItemOnStack
        );
    }

    #[test]
    fn fail_unended_if() {
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::True)
                    .push(OpFrame::OpIf),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::UnexpectedEOF
        );

        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::False)
                    .push(OpFrame::OpIf),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::UnexpectedEOF
        );
    }

    #[test]
    fn checksig_pubkey_into_script_converted() {
        let key = KeyPair::gen();
        let script: Script = key.0.clone().into();

        let engine = {
            let engine = TestEngine::new();
            let mut args = vec![];
            args.push_scripthash(&engine.to_addr.clone().0.into());
            args.push_asset("10.00000 TEST".parse().unwrap());
            let tx = engine.new_transfer_tx(script.clone(), 0, args, &[key]);
            engine.init_direct(tx, script)
        };
        let from_entry = engine.to_transfer_entry("10.00000 TEST");
        assert_eq!(engine.eval().unwrap(), vec![from_entry]);
    }

    #[test]
    fn checksig() {
        let key = KeyPair::gen();
        let mut engine = TestEngine::new().init_with_signers(
            &[key.clone()],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key.0.clone()))
                    .push(OpFrame::OpCheckSig),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let other = KeyPair::gen();
        let mut engine = TestEngine::new().init_with_signers(
            &[key.clone()],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(other.0.clone()))
                    .push(OpFrame::OpCheckSig),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[other],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key.0))
                    .push(OpFrame::OpCheckSig),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checkmultisig_equal_threshold() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();

        let mut engine = TestEngine::new().init_with_signers(
            &[key_3.clone(), key_1.clone()],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key_1.0.clone()))
                    .push(OpFrame::PubKey(key_2.0.clone()))
                    .push(OpFrame::PubKey(key_3.0.clone()))
                    .push(OpFrame::OpCheckMultiSig(2, 3)),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
    }

    #[test]
    fn checkmultisig_threshold_unmet() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();

        let mut engine = TestEngine::new().init_with_signers(
            &[key_3.clone(), key_1.clone()],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key_1.0.clone()))
                    .push(OpFrame::PubKey(key_2.0.clone()))
                    .push(OpFrame::PubKey(key_3.0.clone()))
                    .push(OpFrame::OpCheckMultiSig(3, 3)),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checkmultisig_return_true() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        let builder = Builder::new().push(
            FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSig(2, 3)),
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_2.clone(), key_1.clone(), KeyPair::gen()],
            builder.clone(),
        );
        // This should evaluate to true as the threshold is met, and the trailing signatures are
        // ignored by the script engine.
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine =
            TestEngine::new().init_with_signers(&[key_3.clone(), key_1.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine =
            TestEngine::new().init_with_signers(&[key_2.clone(), key_1.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_3.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
    }

    #[test]
    fn checkmultisig_return_false() {
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        let builder = Builder::new().push(
            FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::OpCheckMultiSig(2, 3)),
        );

        let engine = TestEngine::new().init_with_signers(
            &[KeyPair::gen(), key_3.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::ScriptRetFalse);

        let engine = {
            let script = builder.build().unwrap();

            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    nonce: 0,
                    expiry: 1500000,
                    fee: "1.00000 TEST".parse().unwrap(),
                    signature_pairs: vec![SigPair {
                        // Test valid key with invalid signature
                        pub_key: key_3.0.clone(),
                        signature: Signature(sign::Signature([0; sign::SIGNATUREBYTES])),
                    }],
                },
                from: key_1.clone().0.into(),
                script: script.clone(),
                call_fn: 0,
                args: vec![],
                amount: "10.00000 TEST".parse().unwrap(),
                memo: vec![],
            }));
            tx.append_sign(&key_2);
            tx.append_sign(&key_1);

            ScriptEngine::new(tx.precompute(), script)
        };
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::ScriptRetFalse);
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
            .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::PubKey(key_4.0.clone()))
                .push(OpFrame::OpCheckMultiSigFastFail(2, 4))
                .push(OpFrame::PubKey(key_0.0.clone()))
                .push(OpFrame::OpCheckSig));

        let mut engine = TestEngine::new().init_with_signers(
            &[key_3.clone(), key_2.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_3.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_4.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_3.clone(), key_2.clone(), key_0.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_2.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine =
            TestEngine::new().init_with_signers(&[key_2.clone(), key_1.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine = TestEngine::new().init_with_signers(
            &[key_4.clone(), key_3.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine =
            TestEngine::new().init_with_signers(&[key_4.clone(), key_0.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine = TestEngine::new().init(builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
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
            .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::PubKey(key_1.0.clone()))
                .push(OpFrame::PubKey(key_2.0.clone()))
                .push(OpFrame::PubKey(key_3.0.clone()))
                .push(OpFrame::PubKey(key_4.0.clone()))
                .push(OpFrame::OpCheckMultiSig(3, 4))
                .push(OpFrame::PubKey(key_0.0.clone()))
                .push(OpFrame::OpCheckSig));

        let mut engine = TestEngine::new().init_with_signers(
            &[key_2.clone(), key_1.clone(), key_0.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine =
            TestEngine::new().init_with_signers(&[key_2.clone(), key_0.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine =
            TestEngine::new().init_with_signers(&[key_0.clone(), KeyPair::gen()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = TestEngine::new().init_with_signers(&[key_0.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
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
            .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::PubKey(key_0.0.clone()))
                .push(OpFrame::OpCheckSig)
                .push(OpFrame::OpIf)
                    .push(OpFrame::PubKey(key_1.0.clone()))
                    .push(OpFrame::PubKey(key_2.0.clone()))
                    .push(OpFrame::PubKey(key_3.0.clone()))
                    .push(OpFrame::OpCheckMultiSig(2, 3))
                    .push(OpFrame::OpReturn)
                .push(OpFrame::OpEndIf)
                .push(OpFrame::False));

        // Test threshold is met and tx is signed with key_0
        let mut engine = TestEngine::new().init_with_signers(
            &[key_0.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        // Test tx must be signed with key_0 but threshold is met
        let mut engine =
            TestEngine::new().init_with_signers(&[key_1.clone(), key_2.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        // Test multisig threshold not met
        let mut engine =
            TestEngine::new().init_with_signers(&[key_0.clone(), key_1.clone()], builder);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checksig_and_checkmultisig_with_if_not() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();
        #[rustfmt::skip]
        let builder = Builder::new()
            .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
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
                .push(OpFrame::OpCheckMultiSig(2, 3)));

        // Test threshold is met and tx is signed with key_0
        let mut engine = TestEngine::new().init_with_signers(
            &[key_0.clone(), key_2.clone(), key_1.clone()],
            builder.clone(),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        // Test tx must be signed with key_0 but threshold is met
        let mut engine =
            TestEngine::new().init_with_signers(&[key_1.clone(), key_2.clone()], builder.clone());
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        // Test multisig threshold not met
        let mut engine =
            TestEngine::new().init_with_signers(&[key_0.clone(), key_1.clone()], builder);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checksig_and_checkmultisig_with_fast_fail() {
        let key_0 = KeyPair::gen();
        let key_1 = KeyPair::gen();
        let key_2 = KeyPair::gen();
        let key_3 = KeyPair::gen();

        // Test tx must be signed with key_0 but threshold is met
        #[rustfmt::skip]
        let mut engine = TestEngine::new().init_with_signers(
            &[key_1.clone(), key_2.clone()],
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key_0.0.clone()))
                    .push(OpFrame::OpCheckSigFastFail)
                    .push(OpFrame::PubKey(key_1.0.clone()))
                    .push(OpFrame::PubKey(key_2.0.clone()))
                    .push(OpFrame::PubKey(key_3.0.clone()))
                    .push(OpFrame::OpCheckMultiSig(2, 3))),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        // Test multisig threshold not met
        #[rustfmt::skip]
        let mut engine = TestEngine::new().init_with_signers(
            &[key_0.clone(), key_1.clone()],
            Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(key_1.0.clone()))
                    .push(OpFrame::PubKey(key_2.0.clone()))
                    .push(OpFrame::PubKey(key_3.0.clone()))
                    .push(OpFrame::OpCheckMultiSigFastFail(2, 3))
                    .push(OpFrame::PubKey(key_0.0.clone()))
                    .push(OpFrame::OpCheckSig)),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    struct TestEngine<'a> {
        engine: Option<ScriptEngine<'a>>,
        from_addr: KeyPair,
        to_addr: KeyPair,
    }

    impl<'a> TestEngine<'a> {
        fn new() -> Self {
            Self {
                engine: None,
                from_addr: KeyPair::gen(),
                to_addr: KeyPair::gen(),
            }
        }

        fn from_transfer_entry(&self, amt: &str) -> LogEntry {
            let p2sh: ScriptHash = self.from_addr.clone().0.into();
            LogEntry::Transfer(p2sh, amt.parse().unwrap())
        }

        fn to_transfer_entry(&self, amt: &str) -> LogEntry {
            let p2sh: ScriptHash = self.to_addr.clone().0.into();
            LogEntry::Transfer(p2sh, amt.parse().unwrap())
        }

        fn eval(self) -> Result<Vec<LogEntry>, EvalErr> {
            let engine = self.engine.expect("engine not initialized");
            engine.eval()
        }

        fn init(self, b: Builder) -> Self {
            let from_addr = self.from_addr.clone();
            self.init_with_signers(&[from_addr], b)
        }

        fn init_with_signers(self, keys: &[KeyPair], b: Builder) -> Self {
            let script = b.build().unwrap();
            let tx = self.new_transfer_tx(script.clone(), 0, vec![], keys);
            self.init_direct(tx, script)
        }

        fn init_direct(mut self, tx: TxVariant, script: Script) -> Self {
            self.engine = Some(ScriptEngine::new(tx.precompute(), script));
            self
        }

        fn new_transfer_tx(
            &self,
            script: Script,
            call_fn: u8,
            args: Vec<u8>,
            keys: &[KeyPair],
        ) -> TxVariant {
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    nonce: 0,
                    expiry: 1500000,
                    fee: "1.00000 TEST".parse().unwrap(),
                    signature_pairs: vec![],
                },
                from: self.from_addr.clone().0.into(),
                script: script.clone(),
                call_fn,
                args,
                amount: "10.00000 TEST".parse().unwrap(),
                memo: vec![],
            }));
            keys.iter().for_each(|key| tx.append_sign(key));
            tx
        }
    }

    impl<'a> Deref for TestEngine<'a> {
        type Target = ScriptEngine<'a>;

        fn deref(&self) -> &Self::Target {
            self.engine.as_ref().expect("engine not initialized")
        }
    }

    impl<'a> DerefMut for TestEngine<'a> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.engine.as_mut().expect("engine not initialized")
        }
    }
}
