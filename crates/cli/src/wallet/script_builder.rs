use godcoin::{prelude::*, script::*};
use std::{error::Error, num::ParseIntError};

#[derive(Clone, Debug)]
pub enum BuildError {
    EmptyScript,
    ScriptSizeOverflow,
    UnknownOp(String),
    MissingArgForOp(String),
    WifError(WifError),
    AssetParseError(AssetError),
    Other(String),
}

pub fn build(ops: &[String]) -> Result<Script, BuildError> {
    let mut builder = Builder::new();

    let mut iter = ops.iter();
    while let Some(op) = iter.next() {
        builder = match op.as_ref() {
            // Push value
            "OP_FALSE" => builder.try_push(OpFrame::False),
            "OP_TRUE" => builder.try_push(OpFrame::True),
            "OP_PUBKEY" => {
                let key = iter.next();
                if let Some(key) = key {
                    let key = PublicKey::from_wif(key).map_err(BuildError::WifError)?;
                    builder.try_push(OpFrame::PubKey(key))
                } else {
                    return Err(BuildError::MissingArgForOp(op.to_owned()));
                }
            }
            "OP_ASSET" => {
                let asset = iter.next();
                if let Some(asset) = asset {
                    let asset = asset.parse().map_err(BuildError::AssetParseError)?;
                    builder.try_push(OpFrame::Asset(asset))
                } else {
                    return Err(BuildError::MissingArgForOp(op.to_owned()));
                }
            }
            // Logic
            "OP_NOT" => builder.try_push(OpFrame::OpNot),
            "OP_IF" => builder.try_push(OpFrame::OpIf),
            "OP_ELSE" => builder.try_push(OpFrame::OpElse),
            "OP_ENDIF" => builder.try_push(OpFrame::OpEndIf),
            "OP_RETURN" => builder.try_push(OpFrame::OpReturn),
            // Crypto
            "OP_CHECKSIG" => builder.try_push(OpFrame::OpCheckSig),
            "OP_CHECKSIGFASTFAIL" => builder.try_push(OpFrame::OpCheckSigFastFail),
            "OP_CHECKMULTISIG" => {
                let threshold = iter
                    .next()
                    .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                    .parse()
                    .map_err(|e: ParseIntError| BuildError::Other(e.description().to_owned()))?;
                let key_count = iter
                    .next()
                    .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                    .parse()
                    .map_err(|e: ParseIntError| BuildError::Other(e.description().to_owned()))?;
                builder.try_push(OpFrame::OpCheckMultiSig(threshold, key_count))
            }
            "OP_CHECKMULTISIGFASTFAIL" => {
                let threshold = iter
                    .next()
                    .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                    .parse()
                    .map_err(|e: ParseIntError| BuildError::Other(e.description().to_owned()))?;
                let key_count = iter
                    .next()
                    .ok_or_else(|| BuildError::MissingArgForOp(op.to_owned()))?
                    .parse()
                    .map_err(|e: ParseIntError| BuildError::Other(e.description().to_owned()))?;
                builder.try_push(OpFrame::OpCheckMultiSigFastFail(threshold, key_count))
            }
            _ => return Err(BuildError::UnknownOp(op.to_owned())),
        }
        .ok_or(BuildError::ScriptSizeOverflow)?;
    }

    if !builder.as_ref().is_empty() {
        Ok(builder.build())
    } else {
        Err(BuildError::EmptyScript)
    }
}
