use sodiumoxide::crypto::sign;
use super::key::PublicKey;

#[derive(Debug, Clone, PartialEq)]
pub struct SigPair {
    pub pub_key: PublicKey,
    pub signature: sign::Signature
}

impl SigPair {
    #[inline]
    pub fn verify(&self, msg: &[u8]) -> bool {
        sign::verify_detached(&self.signature, msg, &self.pub_key.0)
    }
}
