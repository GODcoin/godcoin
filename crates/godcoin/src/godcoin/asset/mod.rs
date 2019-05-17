use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::cmp::max;
use std::str::FromStr;

mod precision;
use self::precision::*;

pub mod cmp;
pub use self::cmp::*;

pub mod error;
pub use self::error::*;

pub const MAX_STR_LEN: usize = 32;
pub const MAX_PRECISION: u8 = 4;

pub const EMPTY_GRAEL: Asset = Asset {
    amount: 0,
    decimals: 0,
};

#[derive(Clone, Debug, Default)]
pub struct Asset {
    pub amount: i64,
    pub decimals: u8,
}

impl Asset {
    #[inline]
    pub fn checked_new(amount: i64, decimals: u8) -> Option<Asset> {
        if decimals > MAX_PRECISION {
            return None;
        }
        Some(Asset { amount, decimals })
    }

    pub fn add(&self, other: &Self) -> Option<Self> {
        let decimals = max(self.decimals, other.decimals);
        let a = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let b = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(Asset {
            amount: a.checked_add(b)?,
            decimals,
        })
    }

    pub fn sub(&self, other: &Self) -> Option<Self> {
        let decimals = max(self.decimals, other.decimals);
        let a = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let b = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(Asset {
            amount: a.checked_sub(b)?,
            decimals,
        })
    }

    pub fn mul(&self, other: &Self, precision: u8) -> Option<Self> {
        if precision > MAX_PRECISION {
            return None;
        }
        let decimals = self.decimals + other.decimals;

        let mul = i128::from(self.amount).checked_mul(i128::from(other.amount))?;
        let final_mul = set_decimals_i128(mul, decimals, precision)?;
        if final_mul > i128::from(::std::i64::MAX) {
            return None;
        }
        Some(Asset {
            amount: final_mul as i64,
            decimals: precision,
        })
    }

    pub fn div(&self, other: &Self, precision: u8) -> Option<Self> {
        if other.amount == 0 || precision > MAX_PRECISION {
            return None;
        }
        let decimals = max(max(self.decimals, other.decimals), precision);
        let a = set_decimals_i64(self.amount, self.decimals, decimals * 2)?;
        let b = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(Asset {
            amount: set_decimals_i64(a.checked_div(b)?, decimals, precision)?,
            decimals: precision,
        })
    }

    pub fn pow(&self, num: u16, precision: u8) -> Option<Self> {
        if precision > MAX_PRECISION {
            return None;
        }
        if num == 0 {
            return Some(Asset {
                amount: set_decimals_i64(1, 0, precision)?,
                decimals: precision,
            });
        }

        let decimals = u16::from(self.decimals).checked_mul(num)?;
        let mut res = BigInt::from(1);
        {
            let mut base = BigInt::from(self.amount);
            let mut exp = num;
            loop {
                if exp & 1 == 1 {
                    res = &res * &base;
                }
                exp >>= 1;
                if exp == 0 {
                    break;
                };
                base = &base * &base;
            }
        }

        res = set_decimals_big(&res, decimals, u16::from(precision));
        Some(Asset {
            amount: res.to_i64()?,
            decimals: precision,
        })
    }
}

impl PartialEq for Asset {
    fn eq(&self, other: &Self) -> bool {
        self.eq(other).unwrap()
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
        s.push_str("GRAEL");
        s
    }
}

impl FromStr for Asset {
    type Err = AssetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_STR_LEN {
            return Err(AssetError {
                kind: AssetErrorKind::StrTooLarge,
            });
        }
        let mut split = s.trim().splitn(2, ' ');

        let amount: i64;
        let mut decimals: u8 = 0;
        match split.next() {
            Some(x) => {
                if let Some(pos) = x.find('.') {
                    let len = x.len() - 1;
                    if pos > 0 {
                        decimals = (len - pos) as u8;
                    } else {
                        decimals = len as u8;
                    }

                    if decimals > MAX_PRECISION {
                        return Err(AssetError {
                            kind: AssetErrorKind::InvalidAmount,
                        });
                    }

                    amount = match x.replace('.', "").parse() {
                        Ok(x) => x,
                        Err(_) => {
                            return Err(AssetError {
                                kind: AssetErrorKind::InvalidAmount,
                            });
                        }
                    }
                } else {
                    amount = match x.parse() {
                        Ok(x) => x,
                        Err(_) => {
                            return Err(AssetError {
                                kind: AssetErrorKind::InvalidAmount,
                            });
                        }
                    }
                }
            }
            None => {
                return Err(AssetError {
                    kind: AssetErrorKind::InvalidFormat,
                });
            }
        };

        match split.next() {
            Some(x) => {
                if x != "GRAEL" {
                    return Err(AssetError {
                        kind: AssetErrorKind::InvalidAssetType,
                    });
                }
            }
            None => {
                return Err(AssetError {
                    kind: AssetErrorKind::InvalidFormat,
                });
            }
        };

        Ok(Asset { amount, decimals })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_input() {
        let c = |asset: Asset, amount: &str, decimals: u8| {
            assert_eq!(asset.amount.to_string(), amount);
            assert_eq!(asset.decimals, decimals);
        };

        c(get_asset("1 GRAEL"), "1", 0);
        c(get_asset("1. GRAEL"), "1", 0);
        c(get_asset(".1 GRAEL"), "1", 1);
        c(get_asset("-.1 GRAEL"), "-1", 1);
        c(get_asset("0.1 GRAEL"), "1", 1);
        c(get_asset("1.0 GRAEL"), "10", 1);
        c(get_asset("0 GRAEL"), "0", 0);
        c(get_asset("-0.0 GRAEL"), "0", 1);
        c(get_asset("-1.0 GRAEL"), "-10", 1);
    }

    #[test]
    fn test_asset_to_str() {
        let c = |asset: Asset, s: &str| {
            assert_eq!(asset.to_string(), s);
        };
        c(get_asset("1.0001 GRAEL"), "1.0001 GRAEL");
        c(get_asset("0.0001 GRAEL"), "0.0001 GRAEL");
        c(get_asset("-0.0001 GRAEL"), "-0.0001 GRAEL");
        c(get_asset(".0001 GRAEL"), "0.0001 GRAEL");
        c(get_asset(".1 GRAEL"), "0.1 GRAEL");
        c(get_asset("1.0 GRAEL"), "1.0 GRAEL");
    }

    #[test]
    fn test_fail_parsing_invalid_input() {
        let c = |asset: &str, err: AssetErrorKind| {
            let e = Asset::from_str(asset).err().unwrap();
            assert_eq!(e.kind, err);
        };

        c("1e10 GRAEL", AssetErrorKind::InvalidAmount);
        c("a100 GRAEL", AssetErrorKind::InvalidAmount);
        c("100a GRAEL", AssetErrorKind::InvalidAmount);

        c(
            "1234567890123456789012345678 GRAEL",
            AssetErrorKind::StrTooLarge,
        );
        c("1", AssetErrorKind::InvalidFormat);

        c("1.0 GRAEL a", AssetErrorKind::InvalidAssetType);
        c("1.0 grael", AssetErrorKind::InvalidAssetType);
    }

    #[test]
    fn test_set_precision() {
        let a = get_asset("1.5678 GRAEL");
        assert_eq!(a.decimals, 4);
        assert_eq!(a.amount.to_string(), "15678");

        let a = a.mul(&get_asset("10000 GRAEL"), 0).unwrap();
        assert_eq!(a.decimals, 0);
        assert_eq!(a.amount.to_string(), "15678");

        let a = a.div(&get_asset("100 GRAEL"), 0).unwrap();
        assert_eq!(a.decimals, 0);
        assert_eq!(a.amount.to_string(), "156");

        let a = a.div(&get_asset("100 GRAEL"), 2).unwrap();
        assert_eq!(a.decimals, 2);
        assert_eq!(a.amount.to_string(), "156");

        let a = a.div(&get_asset("100 GRAEL"), 2).unwrap();
        assert_eq!(a.decimals, 2);
        assert_eq!(a.amount.to_string(), "1");
    }

    #[test]
    fn test_perform_arithmetic() {
        let c = |asset: &Asset, amount: &str| {
            assert_eq!(asset.to_string(), amount);
        };

        let a = get_asset("123.456 GRAEL");
        c(&a.add(&get_asset("2.0 GRAEL")).unwrap(), "125.456 GRAEL");
        c(&a.add(&get_asset("-2.0 GRAEL")).unwrap(), "121.456 GRAEL");
        c(&a.add(&get_asset(".0001 GRAEL")).unwrap(), "123.4561 GRAEL");
        c(&a.sub(&get_asset("2.0 GRAEL")).unwrap(), "121.456 GRAEL");
        c(&a.sub(&get_asset("-2.0 GRAEL")).unwrap(), "125.456 GRAEL");
        c(
            &a.mul(&get_asset("100000.1111 GRAEL"), 4).unwrap(),
            "12345613.7159 GRAEL",
        );
        c(
            &a.mul(&get_asset("-100000.1111 GRAEL"), 4).unwrap(),
            "-12345613.7159 GRAEL",
        );
        c(&a.div(&get_asset("23 GRAEL"), 3).unwrap(), "5.367 GRAEL");
        c(&a.div(&get_asset("-23 GRAEL"), 4).unwrap(), "-5.3676 GRAEL");
        c(&a.pow(2, 4).unwrap(), "15241.3839 GRAEL");
        c(&a.pow(3, 4).unwrap(), "1881640.2952 GRAEL");
        c(&a, "123.456 GRAEL");

        c(
            &get_asset("1.0002 GRAEL").pow(1000, 4).unwrap(),
            "1.2213 GRAEL",
        );
        c(
            &get_asset("10 GRAEL").div(&get_asset("2 GRAEL"), 0).unwrap(),
            "5 GRAEL",
        );
        c(
            &get_asset("5 GRAEL").div(&get_asset("10 GRAEL"), 1).unwrap(),
            "0.5 GRAEL",
        );

        assert!(&a.div(&get_asset("0 GRAEL"), 1).is_none());
    }

    #[test]
    fn test_invalid_arithmetic() {
        let a = &get_asset("10 GRAEL");
        let b = &get_asset("9223372036854775807 GRAEL");

        assert!(a.add(b).is_none());
        assert!(a.mul(&get_asset("-1 GRAEL"), 0).unwrap().sub(b).is_none());
        assert!(a.div(b, 8).is_none());
        assert!(a.mul(b, 8).is_none());
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}
