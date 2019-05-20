use super::*;
use crate::crypto::{double_sha256, Digest, PublicKey};
use crate::script::Script;

pub const SCRIPT_HASH_BUF_PREFIX: u8 = 0x03;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScriptHash(Digest);

impl ScriptHash {
    #[inline]
    pub fn from_slice(slice: &[u8]) -> Option<ScriptHash> {
        let digest = Digest::from_slice(slice)?;
        Some(ScriptHash(digest))
    }
}

impl Wif<ScriptHash, Box<str>> for ScriptHash {
    fn from_wif(s: &str) -> Result<ScriptHash, WifError> {
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
        } else if raw[0] != SCRIPT_HASH_BUF_PREFIX {
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
        Ok(ScriptHash::from_slice(key).unwrap())
    }

    fn to_wif(&self) -> Box<str> {
        let mut buf: Vec<u8> = Vec::<u8>::with_capacity(37);
        buf.push(SCRIPT_HASH_BUF_PREFIX);
        buf.extend_from_slice(self.0.as_ref());

        let checksum = &double_sha256(&buf)[0..4];
        buf.extend_from_slice(checksum);

        let mut s = bs58::encode(buf).into_string();
        s.insert_str(0, PUB_ADDRESS_PREFIX);
        s.into_boxed_str()
    }
}

impl From<&Script> for ScriptHash {
    fn from(script: &Script) -> ScriptHash {
        let hash = double_sha256(script);
        ScriptHash(hash)
    }
}

impl From<Script> for ScriptHash {
    fn from(script: Script) -> ScriptHash {
        (&script).into()
    }
}

impl From<PublicKey> for ScriptHash {
    fn from(key: PublicKey) -> ScriptHash {
        let script: Script = key.into();
        script.into()
    }
}

impl From<&PublicKey> for ScriptHash {
    fn from(key: &PublicKey) -> ScriptHash {
        let script: Script = key.clone().into();
        script.into()
    }
}

impl AsRef<[u8]> for ScriptHash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
