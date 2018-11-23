use std::io::{Read, Cursor, Error, ErrorKind};
use sodiumoxide::crypto::sign::Signature;

use crate::crypto::{SigPair, PublicKey, ScriptHash};
use crate::asset::{Asset, AssetSymbol};

pub trait BufWrite {
    fn push_u16(&mut self, num: u16);
    fn push_u32(&mut self, num: u32);
    fn push_i64(&mut self, num: i64);
    fn push_u64(&mut self, num: u64);
    fn push_bytes(&mut self, slice: &[u8]);
    fn push_pub_key(&mut self, key: &PublicKey);
    fn push_script_hash(&mut self, hash: &ScriptHash);
    fn push_sig_pair(&mut self, pair: &SigPair);
    fn push_asset(&mut self, asset: &Asset);
}

impl BufWrite for Vec<u8> {
    fn push_u16(&mut self, num: u16) {
        self.push((num >> 8) as u8);
        self.push(num as u8);
    }

    fn push_u32(&mut self, num: u32) {
        self.push((num >> 24) as u8);
        self.push((num >> 16) as u8);
        self.push((num >> 8) as u8);
        self.push(num as u8);
    }

    fn push_i64(&mut self, num: i64) {
        self.push((num >> 56) as u8);
        self.push((num >> 48) as u8);
        self.push((num >> 40) as u8);
        self.push((num >> 32) as u8);
        self.push((num >> 24) as u8);
        self.push((num >> 16) as u8);
        self.push((num >> 8) as u8);
        self.push(num as u8);
    }

    fn push_u64(&mut self, num: u64) {
        self.push((num >> 56) as u8);
        self.push((num >> 48) as u8);
        self.push((num >> 40) as u8);
        self.push((num >> 32) as u8);
        self.push((num >> 24) as u8);
        self.push((num >> 16) as u8);
        self.push((num >> 8) as u8);
        self.push(num as u8);
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
        self.push_bytes(key.as_ref());
    }

    fn push_script_hash(&mut self, hash: &ScriptHash) {
        self.push_bytes(hash.as_ref());
    }

    fn push_sig_pair(&mut self, pair: &SigPair) {
        self.push_pub_key(&pair.pub_key);
        self.push_bytes(pair.signature.as_ref());
    }

    fn push_asset(&mut self, asset: &Asset) {
        self.push_i64(asset.amount);
        self.push(asset.decimals);
        self.push(asset.symbol as u8);
    }
}

pub trait BufRead {
    fn take_u8(&mut self) -> Result<u8, Error>;
    fn take_u16(&mut self) -> Result<u16, Error>;
    fn take_u32(&mut self) -> Result<u32, Error>;
    fn take_i64(&mut self) -> Result<i64, Error>;
    fn take_u64(&mut self) -> Result<u64, Error>;
    fn take_bytes(&mut self) -> Result<Vec<u8>, Error>;
    fn take_pub_key(&mut self) -> Result<PublicKey, Error>;
    fn take_script_hash(&mut self) -> Result<ScriptHash, Error>;
    fn take_sig_pair(&mut self) -> Result<SigPair, Error>;
    fn take_asset(&mut self) -> Result<Asset, Error>;
}

impl<T: AsRef<[u8]> + Read> BufRead for Cursor<T> {
    fn take_u8(&mut self) -> Result<u8, Error> {
        let mut buf = [0u8;1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn take_u16(&mut self) -> Result<u16, Error> {
        let mut buf = [0u8;2];
        self.read_exact(&mut buf)?;
        Ok((u16::from(buf[0]) << 8) | u16::from(buf[1]))
    }

    fn take_u32(&mut self) -> Result<u32, Error> {
        let mut buf = [0u8;4];
        self.read_exact(&mut buf)?;
        Ok(u32_from_buf!(buf))
    }

    fn take_i64(&mut self) -> Result<i64, Error> {
        let mut buf = [0u8;8];
        self.read_exact(&mut buf)?;
        Ok((i64::from(buf[0]) << 56)
                | (i64::from(buf[1]) << 48)
                | (i64::from(buf[2]) << 40)
                | (i64::from(buf[3]) << 32)
                | (i64::from(buf[4]) << 24)
                | (i64::from(buf[5]) << 16)
                | (i64::from(buf[6]) << 8)
                | i64::from(buf[7]))
    }

    fn take_u64(&mut self) -> Result<u64, Error> {
        let mut buf = [0u8;8];
        self.read_exact(&mut buf)?;
        Ok(u64_from_buf!(buf))
    }

    fn take_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let len = self.take_u32()? as usize;
        let mut buf = Vec::with_capacity(len);
        unsafe { buf.set_len(len); }
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn take_pub_key(&mut self) -> Result<PublicKey, Error> {
        let buf = self.take_bytes()?;
        PublicKey::from_slice(&buf).ok_or_else(|| {
            Error::new(ErrorKind::Other, "incorrect public key length")
        })
    }

    fn take_script_hash(&mut self) -> Result<ScriptHash, Error> {
        let buf = self.take_bytes()?;
        ScriptHash::from_slice(&buf).ok_or_else(|| {
            Error::new(ErrorKind::Other, "incorrect script hash length")
        })
    }

    fn take_sig_pair(&mut self) -> Result<SigPair, Error> {
        let pub_key = self.take_pub_key()?;
        let signature = Signature::from_slice(&self.take_bytes()?).ok_or_else(|| {
            Error::new(ErrorKind::Other, "incorrect signature length")
        })?;
        Ok(SigPair {
            pub_key,
            signature
        })
    }

    fn take_asset(&mut self) -> Result<Asset, Error> {
        let amount = self.take_i64()?;
        let decimals = self.take_u8()?;
        let symbol = match self.take_u8()? {
            0 => AssetSymbol::GOLD,
            1 => AssetSymbol::SILVER,
            _ => return Err(Error::new(ErrorKind::Other, "invalid symbol"))
        };
        Asset::checked_new(amount, decimals, symbol).ok_or_else(|| {
            Error::new(ErrorKind::Other, "invalid asset")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_u32_serialization() {
        let num: u32 = 0x0A0B0C0D;
        let mut v = Vec::with_capacity(4);
        v.push_u32(num);
        assert_eq!(v, [0x0A, 0x0B, 0x0C, 0x0D]);
    }

    #[test]
    fn test_u64_serialization() {
        let num: u64 = 0x0A0B0C0D_0A0B0C0D;
        let mut v = Vec::with_capacity(8);
        v.push_u64(num);

        assert_eq!(v, [0x0A, 0x0B, 0x0C, 0x0D, 0x0A, 0x0B, 0x0C, 0x0D]);
        let dec = Cursor::<&[u8]>::new(&v).take_u64().unwrap();
        assert_eq!(num, dec);
    }

    #[test]
    fn test_asset_serialization() {
        {
            let a = Asset::from_str("12.34 GOLD").unwrap();
            let mut v = vec![];
            v.push_asset(&a);

            let mut c = Cursor::<&[u8]>::new(&v);
            let b = c.take_asset().unwrap();
            assert_eq!(a.to_string(), b.to_string());
        }
        {
            let a = Asset {
                amount: 1,
                decimals: crate::asset::MAX_PRECISION + 1,
                symbol: crate::asset::AssetSymbol::GOLD
            };
            let mut v = vec![];
            v.push_asset(&a);

            let mut c = Cursor::<&[u8]>::new(&v);
            let b = c.take_asset();
            assert!(b.is_err());
        }
    }
}
