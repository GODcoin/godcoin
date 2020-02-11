use godcoin::{prelude::*, script::*};
use std::{error::Error, num::ParseIntError};

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

pub fn build(ops: &[String]) -> Result<Script, BuildError> {
    let mut builder = Builder::new();
    let mut fn_builder = None;
    let mut fn_id = 0u8;

    let mut iter = ops.iter();
    while let Some(op) = iter.next() {
        // Handle function definition
        if op == "OP_DEFINE" {
            if let Some(fnb) = fn_builder {
                builder = builder.push(fnb);
            }
            fn_builder = Some(FnBuilder::new(fn_id, OpFrame::OpDefine));
            fn_id += 1;
            continue;
        }
        match fn_builder {
            Some(builder) => {
                fn_builder = Some(match op.as_ref() {
                    // Push value
                    "OP_FALSE" => builder.push(OpFrame::False),
                    "OP_TRUE" => builder.push(OpFrame::True),
                    "OP_PUBKEY" => match iter.next() {
                        Some(key) => {
                            let key = PublicKey::from_wif(key).map_err(BuildError::WifError)?;
                            builder.push(OpFrame::PubKey(key))
                        }
                        None => return Err(BuildError::MissingArgForOp(op.to_owned())),
                    },
                    "OP_SCRIPTHASH" => match iter.next() {
                        Some(hash) => {
                            let hash = ScriptHash::from_wif(hash).map_err(BuildError::WifError)?;
                            builder.push(OpFrame::ScriptHash(hash))
                        }
                        None => return Err(BuildError::MissingArgForOp(op.to_owned())),
                    },
                    "OP_ASSET" => match iter.next() {
                        Some(asset) => {
                            let asset = asset.parse().map_err(BuildError::AssetParseError)?;
                            builder.push(OpFrame::Asset(asset))
                        }
                        None => return Err(BuildError::MissingArgForOp(op.to_owned())),
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
                    "OP_CHECKSIG" => builder.push(OpFrame::OpCheckSig),
                    "OP_CHECKSIGFASTFAIL" => builder.push(OpFrame::OpCheckSigFastFail),
                    "OP_CHECKMULTISIG" => {
                        let threshold = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                            .parse()
                            .map_err(|e: ParseIntError| {
                                BuildError::Other(e.description().to_owned())
                            })?;
                        let key_count = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                            .parse()
                            .map_err(|e: ParseIntError| {
                                BuildError::Other(e.description().to_owned())
                            })?;
                        builder.push(OpFrame::OpCheckMultiSig(threshold, key_count))
                    }
                    "OP_CHECKMULTISIGFASTFAIL" => {
                        let threshold = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                            .parse()
                            .map_err(|e: ParseIntError| {
                                BuildError::Other(e.description().to_owned())
                            })?;
                        let key_count = iter
                            .next()
                            .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                            .parse()
                            .map_err(|e: ParseIntError| {
                                BuildError::Other(e.description().to_owned())
                            })?;
                        builder.push(OpFrame::OpCheckMultiSigFastFail(threshold, key_count))
                    }
                    _ => return Err(BuildError::UnknownOp(op.to_owned())),
                })
            }
            None => return Err(BuildError::ExpectedFnDefinition),
        }
    }

    builder
        .build()
        .map_err(|total_bytes| BuildError::ScriptSizeOverflow(total_bytes))
}
