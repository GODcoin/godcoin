use super::{op::*, Script};
use crate::{constants::MAX_SCRIPT_BYTE_SIZE, serializer::*};

type FnRef = (u8, u32); // ID, pointer

#[derive(Clone, Debug, Default)]
pub struct Builder {
    lookup_table: Vec<FnRef>,
    body: Vec<u8>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            lookup_table: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Returns the script on success, otherwise an error with the total script size that has exceeded the max script
    /// byte size.
    pub fn build(self) -> Result<Script, usize> {
        // 1 byte for fn len, 5 bytes for 1 byte id + 4 bytes pointer per fn
        let header_len = 1 + (self.lookup_table.len() * 5);
        let total_len = header_len + self.body.len();
        if total_len > MAX_SCRIPT_BYTE_SIZE {
            return Err(total_len);
        }
        let mut script = Vec::<u8>::with_capacity(total_len);
        debug_assert!(self.lookup_table.len() <= u8::max_value() as usize);
        script.push(self.lookup_table.len() as u8);
        for fn_ref in self.lookup_table {
            script.push(fn_ref.0);
            // Offset the byte pointer by the length of the header
            script.push_u32(header_len as u32 + fn_ref.1);
        }
        script.extend(self.body);
        debug_assert_eq!(
            script.len(),
            total_len,
            "buffer capacity under utilized, total length is incorrect"
        );
        debug_assert_eq!(
            script.capacity(),
            total_len,
            "additional allocation was performed, total length is incorrect"
        );
        Ok(Script::new(script))
    }

    pub fn push(mut self, function: FnBuilder) -> Self {
        if self.lookup_table.len() + 1 > usize::from(u8::max_value()) {
            panic!("cannot push more than {} functions", u8::max_value());
        }
        let byte_pos = self.body.len() as u32;
        self.lookup_table.push((function.id, byte_pos));
        self.body.extend(&function.byte_code);
        self
    }
}

#[derive(Clone, Debug)]
pub struct FnBuilder {
    id: u8,
    byte_code: Vec<u8>,
}

impl FnBuilder {
    /// Creates a function builder with the specified `id` and function definition `fn_def`. The function definition
    /// frame must represent an OpDefine operation.
    pub fn new(id: u8, fn_def: OpFrame) -> Self {
        let mut byte_code = vec![];
        match fn_def {
            OpFrame::OpDefine(args) => {
                assert!(
                    args.len() <= usize::from(u8::max_value()),
                    "too many arguments provided"
                );
                byte_code.push(Operand::OpDefine.into());
                byte_code.push(args.len() as u8);
                for arg in args {
                    byte_code.push(arg.into());
                }
            }
            _ => panic!("expected a function definition"),
        }
        Self { id, byte_code }
    }

    pub fn push(mut self, frame: OpFrame) -> Self {
        match frame {
            // Function definition
            OpFrame::OpDefine(_) => panic!("OpDefine cannot be pushed in a function"),
            // Events
            OpFrame::OpTransfer => self.byte_code.push(Operand::OpTransfer.into()),
            OpFrame::OpDestroy => self.byte_code.push(Operand::OpDestroy.into()),
            // Push value
            OpFrame::False => self.byte_code.push(Operand::PushFalse.into()),
            OpFrame::True => self.byte_code.push(Operand::PushTrue.into()),
            OpFrame::AccountId(acc) => {
                self.byte_code.push(Operand::PushAccountId.into());
                self.byte_code.extend(&acc.to_be_bytes());
            }
            OpFrame::Asset(asset) => {
                self.byte_code.push(Operand::PushAsset.into());
                self.byte_code.extend(&asset.amount.to_be_bytes());
            }
            // Arithmetic
            OpFrame::OpLoadAmt => self.byte_code.push(Operand::OpLoadAmt.into()),
            OpFrame::OpLoadRemAmt => self.byte_code.push(Operand::OpLoadRemAmt.into()),
            OpFrame::OpAdd => self.byte_code.push(Operand::OpAdd.into()),
            OpFrame::OpSub => self.byte_code.push(Operand::OpSub.into()),
            OpFrame::OpMul => self.byte_code.push(Operand::OpMul.into()),
            OpFrame::OpDiv => self.byte_code.push(Operand::OpDiv.into()),
            // Logic
            OpFrame::OpNot => self.byte_code.push(Operand::OpNot.into()),
            OpFrame::OpIf => self.byte_code.push(Operand::OpIf.into()),
            OpFrame::OpElse => self.byte_code.push(Operand::OpElse.into()),
            OpFrame::OpEndIf => self.byte_code.push(Operand::OpEndIf.into()),
            OpFrame::OpReturn => self.byte_code.push(Operand::OpReturn.into()),
            OpFrame::OpAbort => self.byte_code.push(Operand::OpAbort.into()),
            // Crypto
            OpFrame::OpCheckPerms => self.byte_code.push(Operand::OpCheckPerms.into()),
            OpFrame::OpCheckPermsFastFail => {
                self.byte_code.push(Operand::OpCheckPermsFastFail.into());
            }
            OpFrame::OpCheckMultiPerms(threshold, key_count) => {
                self.byte_code
                    .extend(&[Operand::OpCheckMultiPerms.into(), threshold, key_count]);
            }
            OpFrame::OpCheckMultiPermsFastFail(threshold, key_count) => self.byte_code.extend(&[
                Operand::OpCheckMultiPermsFastFail.into(),
                threshold,
                key_count,
            ]),
        }
        self
    }
}
