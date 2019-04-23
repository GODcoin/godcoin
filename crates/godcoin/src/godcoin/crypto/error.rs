use std::error::Error;

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

impl Error for WifError {
    fn description(&self) -> &str {
        match self.kind {
            WifErrorKind::InvalidLen => "invalid length",
            WifErrorKind::InvalidPrefix => "invalid prefix",
            WifErrorKind::InvalidChecksum => "invalid checksum",
            WifErrorKind::InvalidBs58Encoding => "invalid bs58 encoding",
        }
    }

    fn cause(&self) -> Option<&Error> {
        None
    }
}

impl std::fmt::Display for WifError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}
