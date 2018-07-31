use ::sodiumoxide::crypto::hash::sha256;
use ::sodiumoxide::crypto::sign;
use ::sodiumoxide::randombytes;
use ::bs58;

const PUB_ADDRESS_PREFIX: &str = "GOD";
const PRIV_BUF_PREFIX: u8 = 0x01;
const PUB_BUF_PREFIX: u8 = 0x02;

pub trait Wif<T> {
    fn from_wif(s: &str) -> Option<T>;
    fn to_wif(&self) -> Box<str>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    key: sign::PublicKey
}

impl PublicKey {
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.key.as_ref()
    }

    #[inline]
    pub fn verify(&self, msg: &[u8], sig: &sign::Signature) -> bool {
        sign::verify_detached(sig, msg, &self.key)
    }

    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Option<PublicKey> {
        let key = sign::PublicKey::from_slice(bytes)?;
        Some(PublicKey { key })
    }

    #[inline]
    pub fn verify_sig_pair(pair: &SigPair, msg: &[u8]) -> bool {
        sign::verify_detached(&pair.signature, msg, &pair.pub_key.key)
    }
}

impl Wif<PublicKey> for PublicKey {
    fn from_wif(s: &str) -> Option<PublicKey> {
        if s.len() < 3 || &s[0..3] != PUB_ADDRESS_PREFIX { return None }
        let raw = bs58::decode(&s[3..]).into_vec().ok()?;
        if raw.len() != 37 || raw[0] != PUB_BUF_PREFIX { return None }

        let prefixed_key = &raw[0..raw.len() - 4];
        {
            let checksum_a = &raw[raw.len() - 4 .. raw.len()];
            let checksum_b = &double_sha256(prefixed_key)[0..4];
            if checksum_a != checksum_b { return None }
        }

        let key = &prefixed_key[1 .. prefixed_key.len()];
        Some(PublicKey {
            key: sign::PublicKey::from_slice(key)?
        })
    }

    fn to_wif(&self) -> Box<str> {
        let mut buf: Vec<u8> = Vec::<u8>::with_capacity(37);
        buf.push(PUB_BUF_PREFIX);
        buf.extend_from_slice(self.key.as_ref());

        let checksum = &double_sha256(&buf)[0..4];
        buf.extend_from_slice(checksum);

        let mut s = bs58::encode(buf).into_string();
        s.insert_str(0, PUB_ADDRESS_PREFIX);
        s.into_boxed_str()
    }
}

pub struct PrivateKey {
    seed: sign::Seed,
    key: sign::SecretKey
}

impl PrivateKey {
    #[inline]
    pub fn sign(&self, msg: &[u8]) -> sign::Signature {
        sign::sign_detached(msg, &self.key)
    }
}

impl Wif<KeyPair> for PrivateKey {
    fn from_wif(s: &str) -> Option<KeyPair> {
        let raw = bs58::decode(s).into_vec().ok()?;
        if raw.len() != 37 || raw[0] != PRIV_BUF_PREFIX { return None }

        let key = &raw[0..raw.len() - 4];
        {
            let checksum_a = &raw[raw.len() - 4 .. raw.len()];
            let checksum_b = &double_sha256(key)[0..4];
            if checksum_a != checksum_b { return None }
        }

        let seed = sign::Seed::from_slice(&key[1..])?;
        let (pk, sk) = sign::keypair_from_seed(&seed);
        Some(KeyPair(PublicKey {
            key: pk
        }, PrivateKey {
            seed,
            key: sk
        }))
    }

    fn to_wif(&self) -> Box<str> {
        let mut buf = Vec::<u8>::with_capacity(37);
        buf.push(PRIV_BUF_PREFIX);
        buf.extend_from_slice(&self.seed.0);

        let checksum = &double_sha256(&buf)[0..4];
        buf.extend_from_slice(checksum);

        bs58::encode(buf).into_string().into_boxed_str()
    }
}

pub struct KeyPair(pub PublicKey, pub PrivateKey);

impl KeyPair {
    #[inline]
    pub fn sign(&self, msg: &[u8]) -> SigPair {
        SigPair {
            pub_key: self.0.clone(),
            signature: self.1.sign(msg)
        }
    }

    pub fn gen_keypair() -> KeyPair {
        let mut raw_seed: [u8; sign::SEEDBYTES] = [0; sign::SEEDBYTES];
        randombytes::randombytes_into(&mut raw_seed);
        let seed = sign::Seed::from_slice(&raw_seed).unwrap();
        let (pk, sk) = sign::keypair_from_seed(&seed);
        KeyPair(PublicKey {
            key: pk
        }, PrivateKey {
            seed,
            key: sk
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct SigPair {
    pub pub_key: PublicKey,
    pub signature: sign::Signature
}

#[inline]
pub fn double_sha256(buf: &[u8]) -> sha256::Digest {
    sha256::hash(sha256::hash(buf).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_recover_keys() {
        let kp = KeyPair::gen_keypair();

        let pk = &*kp.0.to_wif();
        assert_eq!(&*PublicKey::from_wif(pk).unwrap().to_wif(), pk);

        let sk = &*kp.1.to_wif();
        assert_eq!(&*PrivateKey::from_wif(sk).unwrap().1.to_wif(), sk);
    }

    #[test]
    fn test_import_keys_from_wif() {
        let kp = PrivateKey::from_wif("3GAD3otqozDorfu1iDpMQJ1gzWp8PRFEjVHZivZdedKW3i3KtM").unwrap();
        assert_eq!(&*kp.1.to_wif(), "3GAD3otqozDorfu1iDpMQJ1gzWp8PRFEjVHZivZdedKW3i3KtM");
        assert_eq!(&*kp.0.to_wif(), "GOD52QZDBUStV5CudxvKf6bPsQeN7oeKTkEm2nAU1vAUqNVexGTb8");
    }

    #[test]
    fn test_sign_message() {
        let msg = "Hello world!".as_bytes();
        let kp = KeyPair::gen_keypair();

        let sig = &kp.1.sign(msg);
        assert!(kp.0.verify(msg, sig));

        let pair = SigPair {
            pub_key: kp.0,
            signature: *sig
        };
        assert!(PublicKey::verify_sig_pair(&pair, msg));

        // Test bad keys
        let kp = KeyPair::gen_keypair();
        assert!(!kp.0.verify(msg, sig));
    }
}
