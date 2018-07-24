use sodiumoxide::crypto::sign::Signature;
use crypto::{SigPair, PublicKey};
use std::io::{Read, Cursor};
use std::str::FromStr;
use asset::Asset;

trait DecodeTx<T> {
    /**
     * Returns the deserialized type T and the bytes read. Otherwise None is
     * returned if there's a decoding failure.
     */
    fn decode(c: &mut Cursor<Vec<u8>>) -> Option<(T, u16)>;
}

pub trait BufWrite {
    fn push_u16(&mut self, num: u16);
    fn push_u32(&mut self, num: u32);
    fn push_u64(&mut self, num: u64);
    fn push_bytes(&mut self, slice: &[u8]);
    fn push_pub_key(&mut self, key: &PublicKey);
    fn push_sig_pair(&mut self, pair: &SigPair);
    fn push_asset(&mut self, asset: &Asset);
}

impl BufWrite for Vec<u8> {
    fn push_u16(&mut self, num: u16) {
        self.push((num >> 8) as u8);
        self.push(num as u8);
    }

    fn push_u32(&mut self, num: u32) {
        self.push_u16((num >> 16) as u16);
        self.push_u16(num as u16);
    }

    fn push_u64(&mut self, num: u64) {
        self.push_u32((num >> 32) as u32);
        self.push_u32(num as u32);
    }

    fn push_bytes(&mut self, other: &[u8]) {
        if other.is_empty() {
            self.push_u32(0);
            return
        }
        self.push_u32(other.len() as u32);
        self.extend_from_slice(other);
    }

    fn push_pub_key(&mut self, key: &PublicKey) {
        self.push_bytes(key.as_bytes());
    }

    fn push_sig_pair(&mut self, pair: &SigPair) {
        self.push_pub_key(&pair.pub_key);
        self.push_bytes(pair.signature.as_ref());
    }

    fn push_asset(&mut self, asset: &Asset) {
        self.push_bytes((&*asset.to_str()).as_ref());
    }
}

pub trait BufRead {
    fn take_u8(&mut self) -> Option<u8>;
    fn take_u16(&mut self) -> Option<u16>;
    fn take_u32(&mut self) -> Option<u32>;
    fn take_u64(&mut self) -> Option<u64>;
    fn take_bytes(&mut self) -> Option<Vec<u8>>;
    fn take_pub_key(&mut self) -> Option<PublicKey>;
    fn take_sig_pair(&mut self) -> Option<SigPair>;
    fn take_asset(&mut self) -> Option<Asset>;
}

impl<'a, T: AsRef<[u8]> + Read> BufRead for Cursor<T> {
    fn take_u8(&mut self) -> Option<u8> {
        let mut buf = [0u8;1];
        self.read_exact(&mut buf).ok()?;
        Some(buf[0])
    }

    fn take_u16(&mut self) -> Option<u16> {
        let mut buf = [0u8;2];
        self.read_exact(&mut buf).ok()?;
        Some((u16::from(buf[0]) << 8) | u16::from(buf[1]))
    }

    fn take_u32(&mut self) -> Option<u32> {
        let mut buf = [0u8;4];
        self.read_exact(&mut buf).ok()?;

        Some((u32::from(buf[0]) << 24)
                | (u32::from(buf[1]) << 16)
                | (u32::from(buf[2]) << 8)
                | (u32::from(buf[3])))
    }

    fn take_u64(&mut self) -> Option<u64> {
        Some((u64::from(self.take_u32()?) << 32) | u64::from(self.take_u32()?))
    }

    fn take_bytes(&mut self) -> Option<Vec<u8>> {
        let len = self.take_u32()? as usize;
        let mut buf = Vec::with_capacity(len);
        unsafe { buf.set_len(len); }
        self.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    fn take_pub_key(&mut self) -> Option<PublicKey> {
        let buf = self.take_bytes()?;
        PublicKey::from_bytes(&buf)
    }

    fn take_sig_pair(&mut self) -> Option<SigPair> {
        let pub_key = self.take_pub_key()?;
        let signature = Signature::from_slice(&self.take_bytes()?)?;
        Some(SigPair {
            pub_key,
            signature
        })
    }

    fn take_asset(&mut self) -> Option<Asset> {
        let bytes = self.take_bytes()?;
        let s = String::from_utf8(bytes).ok()?;
        Asset::from_str(&s).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_serialization() {
        let a = Asset::from_str("12.34 GOLD").unwrap();
        let mut v = vec![];
        v.push_asset(&a);

        let mut c = Cursor::<&[u8]>::new(&v);
        let b = c.take_asset().unwrap();
        assert_eq!(a.to_str(), b.to_str());
    }
}
