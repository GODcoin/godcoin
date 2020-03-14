use std::{
    error::Error,
    fmt::{self, Display},
};

#[derive(Clone, Debug, PartialEq)]
pub enum WifErrorKind {
    InvalidLen,
    InvalidPrefix,
    InvalidChecksum,
    InvalidBs58Encoding,
}

#[derive(Clone, Debug)]
pub struct WifError {
    pub kind: WifErrorKind,
}

impl WifError {
    pub fn new(kind: WifErrorKind) -> WifError {
        WifError { kind }
    }
}

impl Error for WifError {}

impl Display for WifError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let desc = match self.kind {
            WifErrorKind::InvalidLen => "invalid length",
            WifErrorKind::InvalidPrefix => "invalid prefix",
            WifErrorKind::InvalidChecksum => "invalid checksum",
            WifErrorKind::InvalidBs58Encoding => "invalid bs58 encoding",
        };
        write!(f, "{}", desc)
    }
}
