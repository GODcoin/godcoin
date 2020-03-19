use sodiumoxide::{crypto::sign, randombytes};
use std::fmt;

use super::{sigpair::*, wif::*, Signature};

#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey(pub(crate) sign::PublicKey);

impl PublicKey {
    #[inline]
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> bool {
        sign::verify_detached(&sig.0, msg, &self.0)
    }

    #[inline]
    pub fn from_slice(bytes: &[u8]) -> Option<PublicKey> {
        let key = sign::PublicKey::from_slice(bytes)?;
        Some(PublicKey(key))
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("PublicKey").field(&self.to_wif()).finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrivateKey {
    pub(crate) seed: sign::Seed,
    pub(crate) key: sign::SecretKey,
}

impl PrivateKey {
    #[inline]
    pub fn sign(&self, msg: &[u8]) -> Signature {
        Signature(sign::sign_detached(msg, &self.key))
    }

    #[inline]
    pub fn from_slice(seed: &[u8], key: &[u8]) -> Option<PrivateKey> {
        Some(PrivateKey {
            seed: sign::Seed::from_slice(seed)?,
            key: sign::SecretKey::from_slice(key)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct KeyPair(pub PublicKey, pub PrivateKey);

impl KeyPair {
    #[inline]
    pub fn sign(&self, msg: &[u8]) -> SigPair {
        SigPair {
            pub_key: self.0.clone(),
            signature: self.1.sign(msg),
        }
    }

    #[inline]
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> bool {
        PublicKey::verify(&self.0, msg, sig)
    }

    pub fn gen() -> KeyPair {
        let mut seed = sign::Seed([0; sign::SEEDBYTES]);
        randombytes::randombytes_into(&mut seed.0);
        assert_ne!(seed.0, [0; sign::SEEDBYTES]);
        let (pk, sk) = sign::keypair_from_seed(&seed);
        KeyPair(PublicKey(pk), PrivateKey { seed, key: sk })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_message() {
        let msg = "Hello world!".as_bytes();
        let kp = KeyPair::gen();

        let sig = kp.1.sign(msg);
        assert!(kp.0.verify(msg, &sig));

        let pair = SigPair {
            pub_key: kp.0,
            signature: sig.clone(),
        };
        assert!(pair.verify(msg));

        // Test bad keys
        let kp = KeyPair::gen();
        assert!(!kp.verify(msg, &sig));
    }
}
