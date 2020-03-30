use super::{double_sha256, key::*};
use crate::{account::AccountId, serializer::BufWrite};
use sodiumoxide::crypto::sign;
use std::{
    convert::TryInto,
    error::Error,
    fmt::{self, Display},
};

pub const PUB_ADDRESS_PREFIX: &str = "GOD";
const PRIV_BUF_PREFIX: u8 = 0x01;
const PUB_BUF_PREFIX: u8 = 0x02;
const ACCOUNT_ID_BUF_PREFIX: u8 = 0x03;

pub trait Wif<T, U> {
    fn from_wif(s: &str) -> Result<T, WifError>;
    fn to_wif(&self) -> U;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifErrorKind {
    InvalidLen,
    InvalidPrefix,
    InvalidChecksum,
    InvalidBs58Encoding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

impl Wif<AccountId, Box<str>> for AccountId {
    fn from_wif(s: &str) -> Result<AccountId, WifError> {
        if s.len() < 3 || &s[0..3] != PUB_ADDRESS_PREFIX {
            return Err(WifError::new(WifErrorKind::InvalidPrefix));
        }
        let raw = match bs58::decode(&s[3..]).into_vec() {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(WifError::new(WifErrorKind::InvalidBs58Encoding));
            }
        };
        if raw.len() != 13 {
            return Err(WifError::new(WifErrorKind::InvalidLen));
        } else if raw[0] != ACCOUNT_ID_BUF_PREFIX {
            return Err(WifError::new(WifErrorKind::InvalidPrefix));
        }

        let prefixed_id = &raw[0..raw.len() - 4];
        {
            let checksum_a = &raw[raw.len() - 4..raw.len()];
            let checksum_b = &double_sha256(prefixed_id)[0..4];
            if checksum_a != checksum_b {
                return Err(WifError::new(WifErrorKind::InvalidChecksum));
            }
        }

        let id = &prefixed_id[1..prefixed_id.len()];
        Ok(u64::from_be_bytes(id.try_into().unwrap()))
    }

    fn to_wif(&self) -> Box<str> {
        let mut buf: Vec<u8> = Vec::<u8>::with_capacity(13);
        buf.push(ACCOUNT_ID_BUF_PREFIX);
        buf.push_u64(*self);

        let checksum = &double_sha256(&buf)[0..4];
        buf.extend_from_slice(checksum);

        let mut s = bs58::encode(buf).into_string();
        s.insert_str(0, PUB_ADDRESS_PREFIX);
        s.into_boxed_str()
    }
}

impl Wif<PublicKey, Box<str>> for PublicKey {
    fn from_wif(s: &str) -> Result<PublicKey, WifError> {
        if s.len() < 3 || &s[0..3] != PUB_ADDRESS_PREFIX {
            return Err(WifError::new(WifErrorKind::InvalidPrefix));
        }
        let raw = match bs58::decode(&s[3..]).into_vec() {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(WifError::new(WifErrorKind::InvalidBs58Encoding));
            }
        };
        if raw.len() != 37 {
            return Err(WifError::new(WifErrorKind::InvalidLen));
        } else if raw[0] != PUB_BUF_PREFIX {
            return Err(WifError::new(WifErrorKind::InvalidPrefix));
        }

        let prefixed_key = &raw[0..raw.len() - 4];
        {
            let checksum_a = &raw[raw.len() - 4..raw.len()];
            let checksum_b = &double_sha256(prefixed_key)[0..4];
            if checksum_a != checksum_b {
                return Err(WifError::new(WifErrorKind::InvalidChecksum));
            }
        }

        let key = &prefixed_key[1..prefixed_key.len()];
        Ok(PublicKey(sign::PublicKey::from_slice(key).unwrap()))
    }

    fn to_wif(&self) -> Box<str> {
        let mut buf: Vec<u8> = Vec::<u8>::with_capacity(37);
        buf.push(PUB_BUF_PREFIX);
        buf.extend_from_slice(self.0.as_ref());

        let checksum = &double_sha256(&buf)[0..4];
        buf.extend_from_slice(checksum);

        let mut s = bs58::encode(buf).into_string();
        s.insert_str(0, PUB_ADDRESS_PREFIX);
        s.into_boxed_str()
    }
}

impl Wif<KeyPair, PrivateWif> for PrivateKey {
    fn from_wif(s: &str) -> Result<KeyPair, WifError> {
        let raw = match bs58::decode(s).into_vec() {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(WifError::new(WifErrorKind::InvalidBs58Encoding));
            }
        };
        if raw.len() != 37 {
            return Err(WifError::new(WifErrorKind::InvalidLen));
        } else if raw[0] != PRIV_BUF_PREFIX {
            return Err(WifError::new(WifErrorKind::InvalidPrefix));
        }

        let key = &raw[0..raw.len() - 4];
        {
            let checksum_a = &raw[raw.len() - 4..raw.len()];
            let checksum_b = &double_sha256(key)[0..4];
            if checksum_a != checksum_b {
                return Err(WifError::new(WifErrorKind::InvalidChecksum));
            }
        }

        let seed = sign::Seed::from_slice(&key[1..]).unwrap();
        let (pk, sk) = sign::keypair_from_seed(&seed);
        Ok(KeyPair(PublicKey(pk), PrivateKey { seed, key: sk }))
    }

    fn to_wif(&self) -> PrivateWif {
        let mut buf = Vec::<u8>::with_capacity(37);
        buf.push(PRIV_BUF_PREFIX);
        buf.extend_from_slice(&self.seed.0);

        let checksum = &double_sha256(&buf)[0..4];
        buf.extend_from_slice(checksum);

        PrivateWif(bs58::encode(buf).into_string().into_boxed_str())
    }
}

pub struct PrivateWif(Box<str>);

impl fmt::Display for PrivateWif {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::ops::Deref for PrivateWif {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl Drop for PrivateWif {
    fn drop(&mut self) {
        let bytes = unsafe { self.0.as_bytes_mut() };
        sodiumoxide::utils::memzero(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_recover_keys() {
        let kp = KeyPair::gen();

        let pk = &*kp.0.to_wif();
        assert_eq!(&*PublicKey::from_wif(pk).unwrap().to_wif(), pk);

        let sk = &*kp.1.to_wif();
        assert_eq!(&*PrivateKey::from_wif(sk).unwrap().1.to_wif(), sk);
    }

    #[test]
    fn import_keys_from_wif() {
        let kp =
            PrivateKey::from_wif("3GAD3otqozDorfu1iDpMQJ1gzWp8PRFEjVHZivZdedKW3i3KtM").unwrap();
        assert_eq!(
            &*kp.1.to_wif(),
            "3GAD3otqozDorfu1iDpMQJ1gzWp8PRFEjVHZivZdedKW3i3KtM"
        );
        assert_eq!(
            &*kp.0.to_wif(),
            "GOD52QZDBUStV5CudxvKf6bPsQeN7oeKTkEm2nAU1vAUqNVexGTb8"
        );
    }

    #[test]
    fn import_account_id_from_wif() {
        assert_eq!(AccountId::from_wif("GODFVarNr3nEqUnvquCn"), Ok(0));
    }

    #[test]
    fn invalid_prefix_account_id() {
        let mut bytes = bs58::decode("FVarNr3nEqUnvquCn").into_vec().unwrap();
        bytes[0] = 255;
        let mut wif = bs58::encode(bytes).into_string();
        wif.insert_str(0, PUB_ADDRESS_PREFIX);
        assert_eq!(
            AccountId::from_wif(&wif).unwrap_err().kind,
            WifErrorKind::InvalidPrefix
        );
    }

    #[test]
    fn invalid_prefix_private_key() {
        let mut bytes = bs58::decode("3GAD3otqozDorfu1iDpMQJ1gzWp8PRFEjVHZivZdedKW3i3KtM")
            .into_vec()
            .unwrap();
        bytes[0] = 255;
        let wif = bs58::encode(bytes).into_string();
        assert_eq!(
            PrivateKey::from_wif(&wif).unwrap_err().kind,
            WifErrorKind::InvalidPrefix
        );
    }

    #[test]
    fn invalid_prefix_public_key() {
        let mut bytes = bs58::decode("52QZDBUStV5CudxvKf6bPsQeN7oeKTkEm2nAU1vAUqNVexGTb8")
            .into_vec()
            .unwrap();
        bytes[0] = 255;
        let mut wif = bs58::encode(bytes).into_string();
        wif.insert_str(0, PUB_ADDRESS_PREFIX);
        assert_eq!(
            PublicKey::from_wif(&wif).unwrap_err().kind,
            WifErrorKind::InvalidPrefix
        );
    }

    #[test]
    fn invalid_checksum_account_id() {
        let mut bytes = bs58::decode("FVarNr3nEqUnvquCn").into_vec().unwrap();
        let len = bytes.len();
        for i in 1..5 {
            bytes[len - i] = 0;
        }
        let mut wif = bs58::encode(bytes).into_string();
        wif.insert_str(0, PUB_ADDRESS_PREFIX);
        assert_eq!(
            AccountId::from_wif(&wif).unwrap_err().kind,
            WifErrorKind::InvalidChecksum
        );
    }

    #[test]
    fn invalid_checksum_private_key() {
        let mut bytes = bs58::decode("3GAD3otqozDorfu1iDpMQJ1gzWp8PRFEjVHZivZdedKW3i3KtM")
            .into_vec()
            .unwrap();
        let len = bytes.len();
        for i in 1..5 {
            bytes[len - i] = 0;
        }
        let wif = bs58::encode(bytes).into_string();
        assert_eq!(
            PrivateKey::from_wif(&wif).unwrap_err().kind,
            WifErrorKind::InvalidChecksum
        );
    }

    #[test]
    fn invalid_checksum_public_key() {
        let mut bytes = bs58::decode("52QZDBUStV5CudxvKf6bPsQeN7oeKTkEm2nAU1vAUqNVexGTb8")
            .into_vec()
            .unwrap();
        let len = bytes.len();
        for i in 1..5 {
            bytes[len - i] = 0;
        }
        let mut wif = bs58::encode(bytes).into_string();
        wif.insert_str(0, PUB_ADDRESS_PREFIX);
        assert_eq!(
            PublicKey::from_wif(&wif).unwrap_err().kind,
            WifErrorKind::InvalidChecksum
        );
    }
}
