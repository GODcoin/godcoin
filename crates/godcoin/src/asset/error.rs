use std::error::Error;

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

impl Error for AssetError {
    fn description(&self) -> &str {
        match self.kind {
            AssetErrorKind::InvalidFormat => "invalid format",
            AssetErrorKind::InvalidAssetType => "invalid asset type",
            AssetErrorKind::InvalidAmount => "invalid amount",
            AssetErrorKind::StrTooLarge => "asset string too large",
        }
    }

    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}
