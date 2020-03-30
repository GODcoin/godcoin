use godcoin::{prelude::*, script::*};
use std::num::ParseIntError;

#[derive(Clone, Debug)]
pub enum BuildError {
    ExpectedFnDefinition,
    ScriptSizeOverflow(usize),
    UnknownOp(String),
    MissingArgForOp(String),
    WifError(WifError),
    AssetParseError(AssetError),
    Other(String),
}

pub fn build(ops: &[&str]) -> Result<Script, BuildError> {
    let mut builder = Builder::new();
    let mut fn_builder = None;
    let mut fn_id = 0u8;

    let mut iter = ops.iter();
    while let Some(op) = iter.next() {
        // Handle function definition
        if op == &"OP_DEFINE" {
            if let Some(fnb) = fn_builder {
                builder = builder.push(fnb);
            }
            // TODO handle op definition arguments in the textual script builder
            fn_builder = Some(FnBuilder::new(fn_id, OpFrame::OpDefine(vec![])));
            fn_id += 1;
            continue;
        }
        match fn_builder {
            Some(builder) => {
                fn_builder = Some(match op.as_ref() {
                    // Events
                    "OP_TRANSFER" => builder.push(OpFrame::OpTransfer),
                    // Push value
                    "OP_FALSE" => builder.push(OpFrame::False),
                    "OP_TRUE" => builder.push(OpFrame::True),
                    "OP_ACCOUNTID" => match iter.next() {
                        Some(id) => {
                            let id = AccountId::from_wif(id).map_err(BuildError::WifError)?;
                            builder.push(OpFrame::AccountId(id))
                        }
                        None => return Err(BuildError::MissingArgForOp(op.to_string())),
                    },
                    "OP_ASSET" => match iter.next() {
                        Some(asset) => {
                            let asset = asset.parse().map_err(BuildError::AssetParseError)?;
                            builder.push(OpFrame::Asset(asset))
                        }
                        None => return Err(BuildError::MissingArgForOp(op.to_string())),
                    },
                    // Arithmetic
                    "OP_LOADAMT" => builder.push(OpFrame::OpLoadAmt),
                    "OP_LOADREMAMT" => builder.push(OpFrame::OpLoadRemAmt),
                    "OP_ADD" => builder.push(OpFrame::OpAdd),
                    "OP_SUB" => builder.push(OpFrame::OpSub),
                    "OP_MUL" => builder.push(OpFrame::OpMul),
                    "OP_DIV" => builder.push(OpFrame::OpDiv),
                    // Logic
                    "OP_NOT" => builder.push(OpFrame::OpNot),
                    "OP_IF" => builder.push(OpFrame::OpIf),
                    "OP_ELSE" => builder.push(OpFrame::OpElse),
                    "OP_ENDIF" => builder.push(OpFrame::OpEndIf),
                    "OP_RETURN" => builder.push(OpFrame::OpReturn),
                    // Crypto
                    "OP_CHECKPERMS" => builder.push(OpFrame::OpCheckPerms),
                    "OP_CHECKPERMSFASTFAIL" => builder.push(OpFrame::OpCheckPermsFastFail),
                    "OP_CHECKMULTIPERMS" => {
                        let threshold = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_string()))?
                            .parse()
                            .map_err(|e: ParseIntError| BuildError::Other(format!("{}", e)))?;
                        let acc_count = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_string()))?
                            .parse()
                            .map_err(|e: ParseIntError| BuildError::Other(format!("{}", e)))?;
                        builder.push(OpFrame::OpCheckMultiPerms(threshold, acc_count))
                    }
                    "OP_CHECKMULTIPERMSFASTFAIL" => {
                        let threshold = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_string()))?
                            .parse()
                            .map_err(|e: ParseIntError| BuildError::Other(format!("{}", e)))?;
                        let acc_count = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_string()))?
                            .parse()
                            .map_err(|e: ParseIntError| BuildError::Other(format!("{}", e)))?;
                        builder.push(OpFrame::OpCheckMultiPermsFastFail(threshold, acc_count))
                    }
                    _ => return Err(BuildError::UnknownOp(op.to_string())),
                })
            }
            None => return Err(BuildError::ExpectedFnDefinition),
        }
    }

    if let Some(fnb) = fn_builder {
        builder = builder.push(fnb);
    }
    builder
        .build()
        .map_err(|total_bytes| BuildError::ScriptSizeOverflow(total_bytes))
}
