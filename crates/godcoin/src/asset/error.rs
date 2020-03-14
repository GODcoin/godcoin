use std::{
    error::Error,
    fmt::{self, Display},
};

#[derive(Clone, Debug, PartialEq)]
pub enum AssetErrorKind {
    InvalidFormat,
    InvalidAssetType,
    InvalidAmount,
    StrTooLarge,
}

#[derive(Clone, Debug)]
pub struct AssetError {
    pub kind: AssetErrorKind,
}

impl Error for AssetError {}

impl Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let desc = match self.kind {
            AssetErrorKind::InvalidFormat => "invalid format",
            AssetErrorKind::InvalidAssetType => "invalid asset type",
            AssetErrorKind::InvalidAmount => "invalid amount",
            AssetErrorKind::StrTooLarge => "asset string too large",
        };
        write!(f, "{}", desc)
    }
}
