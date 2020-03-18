use std::{borrow::Cow, convert::TryInto, mem, sync::Arc};

use super::{stack::*, *};
use crate::{
    account::{AccountId, PermsSigVerifyErr},
    asset::Asset,
    blockchain::{Indexer, LogEntry},
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
    tx_data: Cow<'a, TxPrecompData<'a>>,
    indexer: Arc<Indexer>,
    pos: usize,
    stack: Stack,
    log: Vec<LogEntry>,
    total_amt: Asset,
    remaining_amt: Asset,
}

impl<'a> ScriptEngine<'a> {
    pub fn new<T, S>(tx_data: T, script: S, indexer: Arc<Indexer>) -> Self
    where
        T: Into<Cow<'a, TxPrecompData<'a>>>,
        S: Into<Cow<'a, Script>>,
    {
        let tx_data = tx_data.into();
        let script = script.into();

        let total_amt = match tx_data.tx() {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::TransferTx(tx) => tx.amount,
                _ => Asset::default(),
            },
        };

        Self {
            script,
            tx_data,
            indexer,
            pos: 0,
            stack: Stack::new(),
            log: vec![],
            total_amt,
            remaining_amt: total_amt,
        }
    }

    /// Returns the log the script produces after execution completes. If any error occurs during evaluation, execution
    /// will be aborted and return an error.
    #[inline]
    pub fn eval(mut self) -> Result<Vec<LogEntry>, EvalErr> {
        let fn_id = match self.tx_data.tx() {
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
        self.pos = self
            .script
            .get_fn_ptr(fn_id)
            .map_err(|_| self.new_err(EvalErrType::HeaderReadErr))?
            .ok_or_else(|| self.new_err(EvalErrType::UnknownFn))? as usize;

        {
            let op = self.consume_op()?;
            match op {
                Some(OpFrame::OpDefine(args)) => {
                    let mut bin_args = Cursor::<&[u8]>::new(match self.tx_data.tx() {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(_) => &[],
                            TxVariantV0::MintTx(_) => &[],
                            TxVariantV0::CreateAccountTx(_) => &[],
                            TxVariantV0::TransferTx(tx) => &tx.args,
                        },
                    });
                    for arg in args {
                        match arg {
                            Arg::AccountId => {
                                let id = bin_args
                                    .take_u64()
                                    .map_err(|_| self.new_err(EvalErrType::ArgDeserialization))?;
                                map_err_type!(self, self.stack.push(OpFrame::AccountId(id)))?;
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
                    let transfer_to = map_err_type!(self, self.stack.pop_account_id())?;
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
                OpFrame::AccountId(_) => map_err_type!(self, self.stack.push(op))?,
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
                OpFrame::OpCheckPerms => {
                    let acc = map_err_type!(self, self.stack.pop_account_id())?;
                    let success = self.check_acc_perms(1, &[acc])?;
                    map_err_type!(self, self.stack.push(success))?;
                }
                OpFrame::OpCheckPermsFastFail => {
                    let acc = map_err_type!(self, self.stack.pop_account_id())?;
                    if !self.check_acc_perms(1, &[acc])? {
                        return Err(self.new_err(EvalErrType::ScriptRetFalse));
                    }
                }
                OpFrame::OpCheckMultiPerms(threshold, acc_count) => {
                    let accs = {
                        let mut accs = Vec::with_capacity(usize::from(acc_count));
                        for _ in 0..acc_count {
                            accs.push(map_err_type!(self, self.stack.pop_account_id())?);
                        }
                        accs
                    };
                    let success = self.check_acc_perms(usize::from(threshold), &accs)?;
                    map_err_type!(self, self.stack.push(success))?;
                }
                OpFrame::OpCheckMultiPermsFastFail(threshold, acc_count) => {
                    let accs = {
                        let mut accs = Vec::with_capacity(usize::from(acc_count));
                        for _ in 0..acc_count {
                            accs.push(map_err_type!(self, self.stack.pop_account_id())?);
                        }
                        accs
                    };
                    if !self.check_acc_perms(usize::from(threshold), &accs)? {
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
                match self.tx_data.tx() {
                    TxVariant::V0(tx) => match tx {
                        TxVariantV0::TransferTx(tx) => {
                            log.push(LogEntry::Transfer(tx.from, self.remaining_amt))
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
            o if o == Operand::PushAccountId as u8 => {
                let slice = read_bytes!(self, mem::size_of::<u64>());
                let id = u64::from_be_bytes(slice.try_into().unwrap());
                Ok(Some(OpFrame::AccountId(id)))
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
            o if o == Operand::OpCheckPerms as u8 => Ok(Some(OpFrame::OpCheckPerms)),
            o if o == Operand::OpCheckPermsFastFail as u8 => {
                Ok(Some(OpFrame::OpCheckPermsFastFail))
            }
            o if o == Operand::OpCheckMultiPerms as u8 => {
                let threshold = read_bytes!(self);
                let acc_count = read_bytes!(self);
                Ok(Some(OpFrame::OpCheckMultiPerms(threshold, acc_count)))
            }
            o if o == Operand::OpCheckMultiPermsFastFail as u8 => {
                let threshold = read_bytes!(self);
                let acc_count = read_bytes!(self);
                Ok(Some(OpFrame::OpCheckMultiPermsFastFail(
                    threshold, acc_count,
                )))
            }
            _ => Err(self.new_err(EvalErrType::UnknownOp)),
        }
    }

    fn check_acc_perms(&mut self, threshold: usize, accs: &[AccountId]) -> Result<bool, EvalErr> {
        if threshold == 0 {
            return Ok(true);
        } else if threshold > accs.len() {
            return Ok(false);
        }

        let txid = self.tx_data.txid().as_ref();
        let sigs = self.tx_data.tx().sigs();

        let mut valid_threshold = 0;
        for acc_id in accs {
            let account = self
                .indexer
                .get_account(*acc_id)
                .ok_or_else(|| self.new_err(EvalErrType::AccountNotFound))?;
            match account.permissions.verify(txid, sigs) {
                Ok(_) => {}
                Err(PermsSigVerifyErr::InsufficientThreshold)
                | Err(PermsSigVerifyErr::InvalidSig) => {
                    return Ok(false);
                }
                Err(PermsSigVerifyErr::NoMatchingSigs) => {
                    continue;
                }
            }
            valid_threshold += 1;
        }

        Ok(valid_threshold >= threshold)
    }

    fn new_err(&self, err: EvalErrType) -> EvalErr {
        EvalErr::new(self.pos as u32, err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        account::{Account, Permissions},
        blockchain::WriteBatch,
        crypto::{KeyPair, SigPair, Signature},
        tx::{TransferTx, Tx, TxVariant, TxVariantV0},
    };
    use sodiumoxide::{crypto::sign, randombytes};
    use std::{env, fs, ops::DerefMut, path::PathBuf};

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
                    FnBuilder::new(0, OpFrame::OpDefine(vec![Arg::AccountId]))
                        .push(OpFrame::OpLoadRemAmt)
                        .push(OpFrame::OpTransfer)
                        .push(OpFrame::OpLoadRemAmt)
                        .push(OpFrame::True),
                )
                .build()
                .unwrap();
            let mut args = vec![];
            args.push_u64(engine.to_acc.id);
            let tx = engine.new_transfer_tx(0, args, &[]);

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
                FnBuilder::new(1, OpFrame::OpDefine(vec![Arg::AccountId, Arg::Asset]))
                    .push(OpFrame::True),
            )
            .build()
            .unwrap();
        let id = 1234;
        let asset = "1234.00000 TEST".parse().unwrap();

        let mut engine = {
            let mut args = vec![];
            args.push_u64(id);
            args.push_asset(asset);
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(1, args, &[]);
            engine.init_direct(tx, script)
        };
        assert_eq!(
            engine.call_fn(1).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert_eq!(engine.stack.pop_asset(), Ok(asset));
        assert_eq!(engine.stack.pop_account_id(), Ok(id));
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
            let tx = engine.new_transfer_tx(0, vec![], &[]);
            let engine = engine.init_direct(tx, script.clone());
            assert_eq!(engine.eval().unwrap_err().err, EvalErrType::ScriptRetFalse);
        }
        {
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(1, vec![], &[]);
            let engine = engine.init_direct(tx, script.clone());
            let from_entry = engine.from_transfer_entry("10.00000 TEST");
            assert_eq!(engine.eval().unwrap(), vec![from_entry]);
        }
        {
            let engine = TestEngine::new();
            let tx = engine.new_transfer_tx(2, vec![], &[]);
            let engine = engine.init_direct(tx, script);
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
        let mut engine =
            TestEngine::new()
                .init(Builder::new().push(
                    FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::AccountId(0)),
                ));
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::InvalidItemOnStack
        );
    }

    #[test]
    fn fail_invalid_if_cmp() {
        let mut engine = TestEngine::new().init(
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(0))
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
    fn checkperms_default_account_script() {
        let engine = {
            let engine = TestEngine::new();
            let mut args = vec![];
            args.push_u64(engine.to_acc.id);
            args.push_asset("10.00000 TEST".parse().unwrap());
            let tx = engine.new_transfer_tx(0, args, &[engine.from_key.clone()]);
            let script = engine.from_acc.script.clone();
            engine.init_direct(tx, script)
        };
        let from_entry = engine.to_transfer_entry("10.00000 TEST");
        assert_eq!(engine.eval().unwrap(), vec![from_entry]);
    }

    #[test]
    fn checkperms() {
        {
            // Pass verification with the from key and checking the from account perms
            let engine = TestEngine::new();
            let from_key = engine.from_key.clone();
            let from_acc_id = engine.from_acc.id;
            let mut engine = engine.init_with_signers(
                &[from_key],
                Builder::new().push(
                    FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                        .push(OpFrame::AccountId(from_acc_id))
                        .push(OpFrame::OpCheckPerms),
                ),
            );
            assert_eq!(
                engine.call_fn(0).unwrap(),
                vec![engine.from_transfer_entry("10.00000 TEST")]
            );
        }

        {
            // Fail verification as the "to" account ID being checked doesn't meet the signatory
            // threshold
            let engine = TestEngine::new();
            let from_key = engine.from_key.clone();
            let to_acc_id = engine.to_acc.id;
            let mut engine = engine.init_with_signers(
                &[from_key],
                Builder::new().push(
                    FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                        .push(OpFrame::AccountId(to_acc_id))
                        .push(OpFrame::OpCheckPerms),
                ),
            );
            assert_eq!(
                engine.call_fn(0).unwrap_err().err,
                EvalErrType::ScriptRetFalse
            );
        }

        {
            // Fail verification as the "from" account ID being checked doesn't meet the signatory
            // threshold
            let engine = TestEngine::new();
            let to_key = engine.to_key.clone();
            let from_acc_id = engine.from_acc.id;
            let mut engine = engine.init_with_signers(
                &[to_key],
                Builder::new().push(
                    FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                        .push(OpFrame::AccountId(from_acc_id))
                        .push(OpFrame::OpCheckPerms),
                ),
            );
            assert_eq!(
                engine.call_fn(0).unwrap_err().err,
                EvalErrType::ScriptRetFalse
            );
        }
    }

    #[test]
    fn checkmultiperms_equal_threshold() {
        let engine = TestEngine::new();
        let (acc_1, key_1) = engine.create_account(11);
        let (acc_2, _) = engine.create_account(12);
        let (acc_3, key_3) = engine.create_account(13);

        let mut engine = engine.init_with_signers(
            &[key_3.clone(), key_1.clone()],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(acc_1.id))
                    .push(OpFrame::AccountId(acc_2.id))
                    .push(OpFrame::AccountId(acc_3.id))
                    .push(OpFrame::OpCheckMultiPerms(2, 3)),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
    }

    #[test]
    fn checkmultiperms_threshold_unmet() {
        let engine = TestEngine::new();
        let (acc_1, key_1) = engine.create_account(11);
        let (acc_2, _) = engine.create_account(12);
        let (acc_3, key_3) = engine.create_account(13);

        let mut engine = engine.init_with_signers(
            &[key_3.clone(), key_1.clone()],
            Builder::new().push(
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(acc_1.id))
                    .push(OpFrame::AccountId(acc_2.id))
                    .push(OpFrame::AccountId(acc_3.id))
                    .push(OpFrame::OpCheckMultiPerms(3, 3)),
            ),
        );
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checkmultiperms_return_true() {
        let (acc_1, acc_2, acc_3) = (11, 12, 13);
        let builder = Builder::new().push(
            FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::AccountId(acc_1))
                .push(OpFrame::AccountId(acc_2))
                .push(OpFrame::AccountId(acc_3))
                .push(OpFrame::OpCheckMultiPerms(2, 3)),
        );

        {
            let engine = TestEngine::new();
            let (_, key_1) = engine.create_account(acc_1);
            let (_, key_2) = engine.create_account(acc_2);
            let (_, _) = engine.create_account(acc_3);

            let mut engine = engine.init_with_signers(
                &[key_1.clone(), key_2.clone(), KeyPair::gen()],
                builder.clone(),
            );
            // This should evaluate to true as the threshold is met, and the trailing signatures are
            // ignored by the script engine.
            assert_eq!(
                engine.call_fn(0).unwrap(),
                vec![engine.from_transfer_entry("10.00000 TEST")]
            );
        }

        {
            let engine = TestEngine::new();
            let (_, key_1) = engine.create_account(11);
            let (_, _) = engine.create_account(12);
            let (_, key_3) = engine.create_account(13);
            let mut engine = engine.init_with_signers(&[key_3, key_1], builder.clone());
            assert_eq!(
                engine.call_fn(0).unwrap(),
                vec![engine.from_transfer_entry("10.00000 TEST")]
            );
        }

        {
            let engine = TestEngine::new();
            let (_, key_1) = engine.create_account(11);
            let (_, key_2) = engine.create_account(12);
            let (_, _) = engine.create_account(13);
            let mut engine = engine.init_with_signers(&[key_2, key_1], builder.clone());
            assert_eq!(
                engine.call_fn(0).unwrap(),
                vec![engine.from_transfer_entry("10.00000 TEST")]
            );
        }

        {
            let engine = TestEngine::new();
            let (_, key_1) = engine.create_account(11);
            let (_, key_2) = engine.create_account(12);
            let (_, key_3) = engine.create_account(13);
            let mut engine = engine.init_with_signers(&[key_1, key_2, key_3], builder.clone());
            assert_eq!(
                engine.call_fn(0).unwrap(),
                vec![engine.from_transfer_entry("10.00000 TEST")]
            );
        }
    }

    #[test]
    fn checkmultiperms_return_false() {
        let builder = Builder::new().push(
            FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                .push(OpFrame::AccountId(11))
                .push(OpFrame::AccountId(12))
                .push(OpFrame::AccountId(13))
                .push(OpFrame::OpCheckMultiPerms(2, 3)),
        );

        let engine = {
            let script = builder.build().unwrap();
            let engine = TestEngine::new();
            let (_, key_1) = engine.create_account(11);
            let (_, key_2) = engine.create_account(12);
            let (_, key_3) = engine.create_account(13);

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
                from: 0,
                call_fn: 0,
                args: vec![],
                amount: "10.00000 TEST".parse().unwrap(),
                memo: vec![],
            }));
            tx.append_sign(&key_2);
            tx.append_sign(&key_1);

            engine.init_direct(tx, script)
        };
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::ScriptRetFalse);
    }

    #[test]
    fn checkmultiperms_with_trailing_sig_fastfail() {
        fn create_engine<'a>(init_signers: Vec<usize>) -> TestEngine<'a> {
            #[rustfmt::skip]
            let builder = Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(11))
                    .push(OpFrame::AccountId(12))
                    .push(OpFrame::AccountId(13))
                    .push(OpFrame::AccountId(14))
                    .push(OpFrame::OpCheckMultiPermsFastFail(2, 4))
                    .push(OpFrame::AccountId(10))
                    .push(OpFrame::OpCheckPerms));

            let engine = TestEngine::new();
            let keys = [
                engine.create_account(10).1,
                engine.create_account(11).1,
                engine.create_account(12).1,
                engine.create_account(13).1,
                engine.create_account(14).1,
            ];
            let mut signing_keys = Vec::with_capacity(init_signers.len());
            for init in init_signers {
                signing_keys.push(keys[init].clone());
            }
            engine.init_with_signers(&signing_keys, builder)
        }

        let mut engine = create_engine(vec![3, 2, 1, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = create_engine(vec![3, 1, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = create_engine(vec![4, 1, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = create_engine(vec![3, 2, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = create_engine(vec![2, 1, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        let mut engine = create_engine(vec![2, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine = create_engine(vec![4, 3, 2, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine = create_engine(vec![4, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        let mut engine = create_engine(vec![]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checkmultiperms_with_trailing_sig_ignore_multiperms_res() {
        fn create_engine<'a>(init_signers: Vec<usize>) -> TestEngine<'a> {
            #[rustfmt::skip]
            let builder = Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(11))
                    .push(OpFrame::AccountId(12))
                    .push(OpFrame::AccountId(13))
                    .push(OpFrame::AccountId(14))
                    .push(OpFrame::OpCheckMultiPerms(3, 4))
                    .push(OpFrame::AccountId(10))
                    .push(OpFrame::OpCheckPerms));

            let engine = TestEngine::new();
            let keys = [
                engine.create_account(10).1,
                engine.create_account(11).1,
                engine.create_account(12).1,
                engine.create_account(13).1,
                engine.create_account(14).1,
            ];
            let mut signing_keys = Vec::with_capacity(init_signers.len());
            for init in init_signers {
                signing_keys.push(keys[init].clone());
            }
            engine.init_with_signers(&signing_keys, builder)
        }

        let mut engine = create_engine(vec![2, 1, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = create_engine(vec![2, 0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = create_engine(vec![0]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );
        assert!(!engine.stack.pop_bool().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn checkperms_and_checkmultiperms_with_if() {
        fn create_engine<'a>(init_signers: Vec<usize>) -> TestEngine<'a> {
            #[rustfmt::skip]
            let builder = Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(10))
                    .push(OpFrame::OpCheckPerms)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::AccountId(11))
                        .push(OpFrame::AccountId(12))
                        .push(OpFrame::AccountId(13))
                        .push(OpFrame::OpCheckMultiPerms(2, 3))
                        .push(OpFrame::OpReturn)
                    .push(OpFrame::OpEndIf)
                    .push(OpFrame::False));

            let engine = TestEngine::new();
            let keys = [
                engine.create_account(10).1,
                engine.create_account(11).1,
                engine.create_account(12).1,
                engine.create_account(13).1,
            ];
            let mut signing_keys = Vec::with_capacity(init_signers.len());
            for init in init_signers {
                signing_keys.push(keys[init].clone());
            }
            engine.init_with_signers(&signing_keys, builder)
        }

        // Test threshold is met and tx is signed with key_0
        let mut engine = create_engine(vec![0, 2, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        // Test tx must be signed with key_0 but threshold is met
        let mut engine = create_engine(vec![2, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        // Test multisig threshold not met
        let mut engine = create_engine(vec![0, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checkperms_and_checkmultiperms_with_if_not() {
        fn create_engine<'a>(init_signers: Vec<usize>) -> TestEngine<'a> {
            #[rustfmt::skip]
            let builder = Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(10))
                    .push(OpFrame::OpCheckPerms)
                    .push(OpFrame::OpNot)
                    .push(OpFrame::OpIf)
                        .push(OpFrame::False)
                        .push(OpFrame::OpReturn)
                    .push(OpFrame::OpEndIf)
                    .push(OpFrame::AccountId(11))
                    .push(OpFrame::AccountId(12))
                    .push(OpFrame::AccountId(13))
                    .push(OpFrame::OpCheckMultiPerms(2, 3)));

            let engine = TestEngine::new();
            let keys = [
                engine.create_account(10).1,
                engine.create_account(11).1,
                engine.create_account(12).1,
                engine.create_account(13).1,
            ];
            let mut signing_keys = Vec::with_capacity(init_signers.len());
            for init in init_signers {
                signing_keys.push(keys[init].clone());
            }
            engine.init_with_signers(&signing_keys, builder)
        }

        // Test threshold is met and tx is signed with key_0
        let mut engine = create_engine(vec![0, 2, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap(),
            vec![engine.from_transfer_entry("10.00000 TEST")]
        );

        // Test tx must be signed with key_0 but threshold is met
        let mut engine = create_engine(vec![1, 2]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );

        // Test multisig threshold not met
        let mut engine = create_engine(vec![0, 1]);
        assert_eq!(
            engine.call_fn(0).unwrap_err().err,
            EvalErrType::ScriptRetFalse
        );
    }

    #[test]
    fn checkperms_and_checkmultiperms_with_fast_fail() {
        {
            // Test tx must be signed with key_0 but threshold is met
            let engine = TestEngine::new();
            let (acc_0, _) = engine.create_account(10);
            let (acc_1, key_1) = engine.create_account(11);
            let (acc_2, key_2) = engine.create_account(12);
            let (acc_3, _) = engine.create_account(13);

            #[rustfmt::skip]
            let mut engine = engine.init_with_signers(
                &[key_1.clone(), key_2.clone()],
                Builder::new()
                    .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                        .push(OpFrame::AccountId(acc_0.id))
                        .push(OpFrame::OpCheckPermsFastFail)
                        .push(OpFrame::AccountId(acc_1.id))
                        .push(OpFrame::AccountId(acc_2.id))
                        .push(OpFrame::AccountId(acc_3.id))
                        .push(OpFrame::OpCheckMultiPerms(2, 3))),
            );
            assert_eq!(
                engine.call_fn(0).unwrap_err().err,
                EvalErrType::ScriptRetFalse
            );
        }

        {
            // Test multisig threshold not met
            let engine = TestEngine::new();
            let (acc_0, key_0) = engine.create_account(10);
            let (acc_1, key_1) = engine.create_account(11);
            let (acc_2, _) = engine.create_account(12);
            let (acc_3, _) = engine.create_account(13);

            #[rustfmt::skip]
            let mut engine = engine.init_with_signers(
                &[key_0.clone(), key_1.clone()],
                Builder::new()
                    .push(FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                        .push(OpFrame::AccountId(acc_1.id))
                        .push(OpFrame::AccountId(acc_2.id))
                        .push(OpFrame::AccountId(acc_3.id))
                        .push(OpFrame::OpCheckMultiPermsFastFail(2, 3))
                        .push(OpFrame::AccountId(acc_0.id))
                        .push(OpFrame::OpCheckPerms)),
            );
            assert_eq!(
                engine.call_fn(0).unwrap_err().err,
                EvalErrType::ScriptRetFalse
            );
        }
    }

    struct TestEngine<'a> {
        engine: Option<ScriptEngine<'a>>,
        index_path: PathBuf,
        indexer: Arc<Indexer>,
        from_acc: Account,
        from_key: KeyPair,
        to_acc: Account,
        to_key: KeyPair,
    }

    impl<'a> TestEngine<'a> {
        fn new() -> Self {
            let tmp_dir = {
                let mut tmp_dir = env::temp_dir();
                let mut num: [u8; 8] = [0; 8];
                randombytes::randombytes_into(&mut num);
                tmp_dir.push(&format!("godcoin_test_{}", u64::from_be_bytes(num)));
                tmp_dir
            };
            fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));
            let indexer = Arc::new(Indexer::new(&tmp_dir));

            let from_key = KeyPair::gen();
            let from_acc = Account::create_default(
                0,
                Permissions {
                    threshold: 1,
                    keys: vec![from_key.0.clone()],
                },
            );

            let to_key = KeyPair::gen();
            let to_acc = Account::create_default(
                1,
                Permissions {
                    threshold: 1,
                    keys: vec![to_key.0.clone()],
                },
            );

            let mut batch = WriteBatch::new(Arc::clone(&indexer));
            batch.insert_or_update_account(from_acc.clone());
            batch.insert_or_update_account(to_acc.clone());
            batch.commit();

            Self {
                engine: None,
                index_path: tmp_dir,
                indexer,
                from_acc,
                from_key,
                to_acc,
                to_key,
            }
        }

        fn create_account(&self, id: AccountId) -> (Account, KeyPair) {
            let key = KeyPair::gen();
            let acc = Account::create_default(
                id,
                Permissions {
                    threshold: 1,
                    keys: vec![key.0.clone()],
                },
            );

            let mut batch = WriteBatch::new(Arc::clone(&self.indexer));
            batch.insert_or_update_account(acc.clone());
            batch.commit();

            (acc, key)
        }

        fn from_transfer_entry(&self, amt: &str) -> LogEntry {
            let p2a = self.from_acc.id;
            LogEntry::Transfer(p2a, amt.parse().unwrap())
        }

        fn to_transfer_entry(&self, amt: &str) -> LogEntry {
            let p2a = self.to_acc.id;
            LogEntry::Transfer(p2a, amt.parse().unwrap())
        }

        fn eval(mut self) -> Result<Vec<LogEntry>, EvalErr> {
            let mut engine = None;
            mem::swap(&mut engine, &mut self.engine);
            let engine = engine.expect("engine not initialized");
            engine.eval()
        }

        fn init(self, b: Builder) -> Self {
            let from_key = self.from_key.clone();
            self.init_with_signers(&[from_key], b)
        }

        fn init_with_signers(self, keys: &[KeyPair], b: Builder) -> Self {
            let script = b.build().unwrap();
            let tx = self.new_transfer_tx(0, vec![], keys);
            self.init_direct(tx, script)
        }

        fn init_direct(mut self, tx: TxVariant, script: Script) -> Self {
            let indexer = Arc::clone(&self.indexer);
            self.engine = Some(ScriptEngine::new(tx.precompute(), script, indexer));
            self
        }

        fn new_transfer_tx(&self, call_fn: u8, args: Vec<u8>, keys: &[KeyPair]) -> TxVariant {
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    nonce: 0,
                    expiry: 1500000,
                    fee: "1.00000 TEST".parse().unwrap(),
                    signature_pairs: vec![],
                },
                from: 0,
                call_fn,
                args,
                amount: "10.00000 TEST".parse().unwrap(),
                memo: vec![],
            }));
            keys.iter().for_each(|key| tx.append_sign(key));
            tx
        }
    }

    impl<'a> Drop for TestEngine<'a> {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.index_path).expect("Failed to rm dir");
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
