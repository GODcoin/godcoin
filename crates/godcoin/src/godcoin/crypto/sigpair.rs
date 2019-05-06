use super::{PublicKey, Signature};
use sodiumoxide::crypto::sign;

#[derive(Clone, Debug, PartialEq)]
pub struct SigPair {
    pub pub_key: PublicKey,
    pub signature: Signature,
}

impl SigPair {
    #[inline]
    pub fn verify(&self, msg: &[u8]) -> bool {
        sign::verify_detached(&self.signature.0, msg, &self.pub_key.0)
    }
}
