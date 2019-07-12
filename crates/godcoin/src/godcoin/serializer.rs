use sodiumoxide::crypto::{
    hash::sha256::DIGESTBYTES,
    sign::{PUBLICKEYBYTES, SIGNATUREBYTES},
};
use std::io::{Cursor, Error, ErrorKind, Read};

use crate::asset::Asset;
use crate::crypto::{PublicKey, ScriptHash, SigPair, Signature};

macro_rules! read_exact_bytes {
    ($self:expr, $len:expr) => {{
        let mut buf = Vec::with_capacity($len);
        unsafe {
            buf.set_len($len);
        }
        $self.read_exact(&mut buf)?;
        buf
    }};
}

#[inline]
fn zigzag_encode(from: i64) -> u64 {
    ((from << 1) ^ (from >> 63)) as u64
}

#[inline]
fn zigzag_decode(from: u64) -> i64 {
    ((from >> 1) ^ (-((from & 1) as i64)) as u64) as i64
}

pub trait BufWrite {
    fn push_u16(&mut self, num: u16);
    fn push_u32(&mut self, num: u32);
    fn push_i64(&mut self, num: i64);
    fn push_var_i64(&mut self, num: i64);
    fn push_u64(&mut self, num: u64);
    fn push_bytes(&mut self, slice: &[u8]);
    fn push_pub_key(&mut self, key: &PublicKey);
    fn push_script_hash(&mut self, hash: &ScriptHash);
    fn push_sig_pair(&mut self, pair: &SigPair);
    fn push_asset(&mut self, asset: Asset);
}

impl BufWrite for Vec<u8> {
    #[inline]
    fn push_u16(&mut self, num: u16) {
        self.extend(&num.to_be_bytes());
    }

    #[inline]
    fn push_u32(&mut self, num: u32) {
        self.extend(&num.to_be_bytes());
    }

    #[inline]
    fn push_i64(&mut self, num: i64) {
        self.extend(&num.to_be_bytes());
    }

    fn push_var_i64(&mut self, num: i64) {
        let mut more = true;
        let mut num = zigzag_encode(num);

        while more {
            let mut byte: u8 = (num & 0x7F) as u8;
            num >>= 7;

            if num == 0 {
                more = false;
            } else {
                byte |= 0x80;
            }

            self.push(byte);
        }
    }

    #[inline]
    fn push_u64(&mut self, num: u64) {
        self.extend(&num.to_be_bytes());
    }

    fn push_bytes(&mut self, other: &[u8]) {
        if other.is_empty() {
            self.push_u32(0);
            return;
        }
        self.push_u32(other.len() as u32);
        self.extend_from_slice(other);
    }

    fn push_pub_key(&mut self, key: &PublicKey) {
        self.extend_from_slice(key.as_ref());
    }

    fn push_script_hash(&mut self, hash: &ScriptHash) {
        self.extend_from_slice(hash.as_ref());
    }

    fn push_sig_pair(&mut self, pair: &SigPair) {
        self.push_pub_key(&pair.pub_key);
        self.extend_from_slice(pair.signature.as_ref());
    }

    fn push_asset(&mut self, asset: Asset) {
        self.push_var_i64(asset.amount);
    }
}

pub trait BufRead {
    fn take_u8(&mut self) -> Result<u8, Error>;
    fn take_u16(&mut self) -> Result<u16, Error>;
    fn take_u32(&mut self) -> Result<u32, Error>;
    fn take_i64(&mut self) -> Result<i64, Error>;
    fn take_var_i64(&mut self) -> Result<i64, Error>;
    fn take_u64(&mut self) -> Result<u64, Error>;
    fn take_bytes(&mut self) -> Result<Vec<u8>, Error>;
    fn take_pub_key(&mut self) -> Result<PublicKey, Error>;
    fn take_script_hash(&mut self) -> Result<ScriptHash, Error>;
    fn take_sig_pair(&mut self) -> Result<SigPair, Error>;
    fn take_asset(&mut self) -> Result<Asset, Error>;
}

impl<T: AsRef<[u8]> + Read> BufRead for Cursor<T> {
    fn take_u8(&mut self) -> Result<u8, Error> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn take_u16(&mut self) -> Result<u16, Error> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    fn take_u32(&mut self) -> Result<u32, Error> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn take_i64(&mut self) -> Result<i64, Error> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(i64::from_be_bytes(buf))
    }

    fn take_var_i64(&mut self) -> Result<i64, Error> {
        let mut result: u64 = 0;
        let mut shift = 0;
        let mut buf = [0u8; 1];
        loop {
            if shift > 63 {
                return Err(Error::new(ErrorKind::Other, "overflow taking varint"));
            }

            self.read_exact(&mut buf)?;
            let byte = buf[0];

            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }

            shift += 7;
        }

        Ok(zigzag_decode(result))
    }

    fn take_u64(&mut self) -> Result<u64, Error> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }

    fn take_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let len = self.take_u32()? as usize;
        let buf = read_exact_bytes!(self, len);
        Ok(buf)
    }

    fn take_pub_key(&mut self) -> Result<PublicKey, Error> {
        let buf = read_exact_bytes!(self, PUBLICKEYBYTES);
        PublicKey::from_slice(&buf)
            .ok_or_else(|| Error::new(ErrorKind::Other, "incorrect public key length"))
    }

    fn take_script_hash(&mut self) -> Result<ScriptHash, Error> {
        let buf = read_exact_bytes!(self, DIGESTBYTES);
        ScriptHash::from_slice(&buf)
            .ok_or_else(|| Error::new(ErrorKind::Other, "incorrect script hash length"))
    }

    fn take_sig_pair(&mut self) -> Result<SigPair, Error> {
        let pub_key = self.take_pub_key()?;
        let signature = {
            let buf = read_exact_bytes!(self, SIGNATUREBYTES);
            Signature::from_slice(&buf)
                .ok_or_else(|| Error::new(ErrorKind::Other, "incorrect signature length"))?
        };
        Ok(SigPair { pub_key, signature })
    }

    fn take_asset(&mut self) -> Result<Asset, Error> {
        let amount = self.take_var_i64()?;
        Ok(Asset::new(amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32_serialization() {
        let num: u32 = 0x0A0B0C0D;
        let mut v = Vec::with_capacity(4);
        v.push_u32(num);
        assert_eq!(v, [0x0A, 0x0B, 0x0C, 0x0D]);
    }

    #[test]
    fn u64_serialization() {
        let num: u64 = 0x0A0B0C0D_0A0B0C0D;
        let mut v = Vec::with_capacity(8);
        v.push_u64(num);

        assert_eq!(v, [0x0A, 0x0B, 0x0C, 0x0D, 0x0A, 0x0B, 0x0C, 0x0D]);
        let dec = Cursor::<&[u8]>::new(&v).take_u64().unwrap();
        assert_eq!(num, dec);
    }

    #[test]
    fn asset_serialization() {
        let a = "12.34567 GRAEL".parse().unwrap();
        let mut v = vec![];
        v.push_asset(a);

        let mut c = Cursor::<&[u8]>::new(&v);
        let b = c.take_asset().unwrap();
        assert_eq!(a.to_string(), b.to_string());
    }

    #[test]
    fn zigzag() {
        fn cmp(decoded: i64, encoded: u64) {
            assert_eq!(decoded, zigzag_decode(encoded));
            assert_eq!(encoded, zigzag_encode(decoded));
        }

        cmp(0, 0);
        cmp(-1, 1);
        cmp(1, 2);
        cmp(-2, 3);
        cmp(2147483647, 4294967294);
        cmp(-2147483648, 4294967295);
        cmp(9223372036854775807, 18446744073709551614);
        cmp(-9223372036854775808, 18446744073709551615);
    }

    #[test]
    fn var_i64_serialization() {
        let mut buf = vec![];
        buf.push_var_i64(0);
        buf.push_var_i64(300);
        buf.push_var_i64(-300);
        buf.push_var_i64(i64::max_value());
        buf.push_var_i64(i64::min_value());
        // Outputs 0 as only the first bit is checked on the final byte with a shift of 63
        buf.extend(vec![
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7E,
        ]);
        // Outputs 1 << 62 as bit 62 is set
        buf.extend(vec![
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
        ]);

        let mut c = Cursor::<&[u8]>::new(&buf);
        assert_eq!(c.take_var_i64().unwrap(), 0);
        assert_eq!(c.take_var_i64().unwrap(), 300);
        assert_eq!(c.take_var_i64().unwrap(), -300);
        assert_eq!(c.take_var_i64().unwrap(), i64::max_value());
        assert_eq!(c.take_var_i64().unwrap(), i64::min_value());
        assert_eq!(c.take_var_i64().unwrap(), 0);
        assert_eq!(c.take_var_i64().unwrap(), 1 << 62);
    }

    #[test]
    fn var_i64_serialization_overflow() {
        use std::error;

        let buf = vec![
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0,
        ];
        let mut c = Cursor::<&[u8]>::new(&buf);
        assert_eq!(
            error::Error::description(&c.take_var_i64().unwrap_err()),
            "overflow taking varint"
        );
    }

    #[test]
    fn var_i64_serialization_eof() {
        let buf = vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80];
        let mut c = Cursor::<&[u8]>::new(&buf);
        assert_eq!(
            c.take_var_i64().unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );
    }
}
