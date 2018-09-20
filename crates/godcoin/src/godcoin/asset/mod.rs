use num_traits::ToPrimitive;
use num_bigint::BigInt;
use std::str::FromStr;
use std::cmp::max;

mod precision;
use self::precision::*;

pub mod asset_symbol;
pub use self::asset_symbol::*;

pub mod cmp;
pub use self::cmp::*;

pub mod error;
pub use self::error::*;

pub const MAX_STR_LEN: usize = 32;
pub const MAX_PRECISION: u8 = 8;

pub const EMPTY_GOLD: Asset = Asset {
    amount: 0,
    decimals: 0,
    symbol: AssetSymbol::GOLD
};

pub const EMPTY_SILVER: Asset = Asset {
    amount: 0,
    decimals: 0,
    symbol: AssetSymbol::SILVER
};

#[derive(Clone, Debug)]
pub struct Asset {
    pub amount: i64,
    pub decimals: u8,
    pub symbol: AssetSymbol
}

impl Asset {
    #[inline(always)]
    pub fn new(amount: i64, decimals: u8, symbol: AssetSymbol) -> Option<Asset> {
        if decimals > MAX_PRECISION { return None }
        Some(Asset {
            amount,
            decimals,
            symbol
        })
    }

    pub fn add(&self, other: &Self) -> Option<Self> {
        if self.symbol != other.symbol { return None }
        let decimals = max(self.decimals, other.decimals);
        let a = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let b = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(Asset { amount: a.checked_add(b)?, decimals, symbol: self.symbol })
    }

    pub fn sub(&self, other: &Self) -> Option<Self> {
        if self.symbol != other.symbol { return None }
        let decimals = max(self.decimals, other.decimals);
        let a = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let b = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(Asset { amount: a.checked_sub(b)?, decimals, symbol: self.symbol })
    }

    pub fn mul(&self, other: &Self, precision: u8) -> Option<Self> {
        if self.symbol != other.symbol || precision > MAX_PRECISION { return None }
        let decimals = self.decimals + other.decimals;

        let mul = i128::from(self.amount).checked_mul(i128::from(other.amount))?;
        let final_mul = set_decimals_i128(mul, decimals, precision)?;
        if final_mul > i128::from(::std::i64::MAX) { return None }
        Some(Asset {
            amount: final_mul as i64,
            decimals: precision,
            symbol: self.symbol
        })
    }

    pub fn div(&self, other: &Self, precision: u8) -> Option<Self> {
        if self.symbol != other.symbol
            || other.amount == 0
            || precision > MAX_PRECISION { return None }
        let decimals = max(max(self.decimals, other.decimals), precision);
        let a = set_decimals_i64(self.amount, self.decimals, decimals * 2)?;
        let b = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(Asset {
            amount: set_decimals_i64(a.checked_div(b)?, decimals, precision)?,
            decimals: precision,
            symbol: self.symbol
        })
    }

    pub fn pow(&self, num: u16, precision: u8) -> Option<Self> {
        if precision > MAX_PRECISION { return None }
        if num == 0 {
            return Some(Asset {
                amount: set_decimals_i64(1, 0, precision)?,
                decimals: precision,
                symbol: self.symbol
            })
        }
        let big_zero = BigInt::from(0);
        let big_one = BigInt::from(1);

        let decimals = u16::from(self.decimals).checked_mul(num)?;
        let mut res = BigInt::from(1);
        {
            let mut base = BigInt::from(self.amount);
            let mut exp = BigInt::from(num);
            loop {
                if &exp & &big_one == big_one { res = &res * &base; }
                exp >>= 1;
                if exp == big_zero { break };
                base = &base * &base;
            }
        }

        res = set_decimals_big(&res, decimals, u16::from(precision));
        Some(Asset {
            amount: res.to_i64()?,
            decimals: precision,
            symbol: self.symbol
        })
    }
}

impl ToString for Asset {
    fn to_string(&self) -> String {
        let mut s = self.amount.to_string();
        if self.decimals > 0 {
            let len = s.len();
            if len < self.decimals as usize {
                let start = if self.amount < 0 { 1 } else { 0 };
                let diff = self.decimals as usize - len + start;
                s.insert_str(start, "0.");
                s.insert_str(start + 2, &"0".repeat(diff));
            } else if len == self.decimals as usize {
                s.insert_str(0, "0.");
            } else {
                s.insert(len - (self.decimals as usize), '.');
            }
        }
        s.push(' ');
        s.push_str(self.symbol.as_str());
        s
    }
}

impl FromStr for Asset {
    type Err = AssetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_STR_LEN {
            return Err(AssetError { kind: AssetErrorKind::StrTooLarge })
        }
        let mut split = s.trim().splitn(2, ' ');

        let amount: i64;
        let mut decimals: u8 = 0;
        match split.next() {
            Some(x) => {
                if let Some(pos) = x.find('.') {
                    let len = x.len() - 1;
                    if pos > 0 { decimals = (len - pos) as u8; }
                    else { decimals = len as u8; }

                    if decimals > MAX_PRECISION {
                        return Err(AssetError { kind: AssetErrorKind::InvalidAmount })
                    }

                    amount = match x.replace('.', "").parse() {
                        Ok(x) => x,
                        Err(_) => return Err(AssetError { kind: AssetErrorKind::InvalidAmount })
                    }
                } else {
                    amount = match x.parse() {
                        Ok(x) => x,
                        Err(_) => return Err(AssetError { kind: AssetErrorKind::InvalidAmount })
                    }
                }
            },
            None => return Err(AssetError { kind: AssetErrorKind::InvalidFormat })
        };

        let symbol = match split.next() {
            Some(x) => {
                match AssetSymbol::parse_str(x) {
                    Some(x) => x,
                    None => return Err(AssetError { kind: AssetErrorKind::InvalidAssetType })
                }
            },
            None => return Err(AssetError { kind: AssetErrorKind::InvalidFormat })
        };

        Ok(Asset { amount, decimals, symbol })
    }
}

#[derive(Clone, Debug)]
pub struct Balance {
    pub gold: Asset,
    pub silver: Asset
}

macro_rules! agnostic_op {
    ($op:ident) => {
        impl Balance {
            #[inline]
            #[must_use = "operation can fail"]
            pub fn $op(&mut self, asset: &Asset) -> Option<&mut Balance> {
                match asset.symbol {
                    AssetSymbol::GOLD => {
                        self.gold = self.gold.$op(asset)?;
                    },
                    AssetSymbol::SILVER => {
                        self.silver = self.silver.$op(asset)?;
                    }
                }
                Some(self)
            }
        }
    };
}

agnostic_op!(add);
agnostic_op!(sub);

impl Default for Balance {
    fn default() -> Balance {
        Balance {
            gold: EMPTY_GOLD,
            silver: EMPTY_SILVER
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_input() {
        let c = |asset: Asset, amount: &str, decimals: u8, symbol: AssetSymbol| {
            assert_eq!(asset.amount.to_string(), amount);
            assert_eq!(asset.decimals, decimals);
            assert_eq!(asset.symbol, symbol);
        };

        c(get_asset("1 GOLD"), "1", 0, AssetSymbol::GOLD);
        c(get_asset("1. GOLD"), "1", 0, AssetSymbol::GOLD);
        c(get_asset(".1 GOLD"), "1", 1, AssetSymbol::GOLD);
        c(get_asset("-.1 GOLD"), "-1", 1, AssetSymbol::GOLD);
        c(get_asset("0.1 GOLD"), "1", 1, AssetSymbol::GOLD);
        c(get_asset("1.0 SILVER"), "10", 1, AssetSymbol::SILVER);
        c(get_asset("0 SILVER"), "0", 0, AssetSymbol::SILVER);
        c(get_asset("-0.0 SILVER"), "0", 1, AssetSymbol::SILVER);
        c(get_asset("-1.0 SILVER"), "-10", 1, AssetSymbol::SILVER);
    }

    #[test]
    fn test_asset_to_str() {
        let c = |asset: Asset, s: &str| {
            assert_eq!(asset.to_string(), s);
        };
        c(get_asset("1.00001 GOLD"), "1.00001 GOLD");
        c(get_asset("0.00001 GOLD"), "0.00001 GOLD");
        c(get_asset("-0.00001 GOLD"), "-0.00001 GOLD");
        c(get_asset(".00001 GOLD"), "0.00001 GOLD");
        c(get_asset(".1 GOLD"), "0.1 GOLD");
        c(get_asset("1.0 GOLD"), "1.0 GOLD");
    }

    #[test]
    fn test_fail_parsing_invalid_input() {
        let c = |asset: &str, err: AssetErrorKind| {
            let e = Asset::from_str(asset).err().unwrap();
            assert_eq!(e.kind, err);
        };

        c("1e10 GOLD", AssetErrorKind::InvalidAmount);
        c("a100 GOLD", AssetErrorKind::InvalidAmount);
        c("100a GOLD", AssetErrorKind::InvalidAmount);

        c("1234567890123456789012345678 GOLD", AssetErrorKind::StrTooLarge);
        c("1", AssetErrorKind::InvalidFormat);

        c("1.0 GOLD a", AssetErrorKind::InvalidAssetType);
        c("1.0 gold", AssetErrorKind::InvalidAssetType);
    }

    #[test]
    fn test_set_precision() {
        let a = get_asset("1.5678 GOLD");
        assert_eq!(a.decimals, 4);
        assert_eq!(a.amount.to_string(), "15678");

        let a = a.mul(&get_asset("10000 GOLD"), 0).unwrap();
        assert_eq!(a.decimals, 0);
        assert_eq!(a.amount.to_string(), "15678");

        let a = a.div(&get_asset("100 GOLD"), 0).unwrap();
        assert_eq!(a.decimals, 0);
        assert_eq!(a.amount.to_string(), "156");

        let a = a.div(&get_asset("100 GOLD"), 2).unwrap();
        assert_eq!(a.decimals, 2);
        assert_eq!(a.amount.to_string(), "156");

        let a = a.div(&get_asset("100 GOLD"), 2).unwrap();
        assert_eq!(a.decimals, 2);
        assert_eq!(a.amount.to_string(), "1");
    }

    #[test]
    fn test_perform_arithmetic() {
        let c = |asset: &Asset, amount: &str| {
            assert_eq!(asset.to_string(), amount);
        };

        let a = get_asset("123.456 GOLD");
        c(&a.add(&get_asset("2.0 GOLD")).unwrap(), "125.456 GOLD");
        c(&a.add(&get_asset("-2.0 GOLD")).unwrap(), "121.456 GOLD");
        c(&a.add(&get_asset(".00000001 GOLD")).unwrap(), "123.45600001 GOLD");
        c(&a.sub(&get_asset("2.0 GOLD")).unwrap(), "121.456 GOLD");
        c(&a.sub(&get_asset("-2.0 GOLD")).unwrap(), "125.456 GOLD");
        c(&a.mul(&get_asset("100000.11111111 GOLD"), 8).unwrap(), "12345613.71733319 GOLD");
        c(&a.mul(&get_asset("-100000.11111111 GOLD"), 8).unwrap(), "-12345613.71733319 GOLD");
        c(&a.div(&get_asset("23 GOLD"), 3).unwrap(), "5.367 GOLD");
        c(&a.div(&get_asset("-23 GOLD"), 8).unwrap(), "-5.36765217 GOLD");
        c(&a.pow(2, 8).unwrap(), "15241.38393600 GOLD");
        c(&a.pow(3, 8).unwrap(), "1881640.29520281 GOLD");
        c(&a, "123.456 GOLD");

        c(&get_asset("1.0002 GOLD").pow(1000, 8).unwrap(), "1.22137833 GOLD");
        c(&get_asset("10 GOLD").div(&get_asset("2 GOLD"), 0).unwrap(), "5 GOLD");
        c(&get_asset("5 GOLD").div(&get_asset("10 GOLD"), 1).unwrap(), "0.5 GOLD");

        assert!(&a.div(&get_asset("0 GOLD"), 1).is_none());
    }

    #[test]
    fn test_invalid_arithmetic() {
        let a = &get_asset("10 GOLD");
        let b = &get_asset("10 SILVER");

        assert!(a.add(b).is_none());
        assert!(a.sub(b).is_none());
        assert!(a.div(b, 8).is_none());
        assert!(a.mul(b, 8).is_none());
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}
