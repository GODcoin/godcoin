use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::{fmt, str::FromStr};

mod precision;
use self::precision::*;

pub mod error;
pub use self::error::*;

#[cfg(not(any(test, feature = "testnet")))]
pub const ASSET_SYMBOL: &str = "GRAEL";

#[cfg(any(test, feature = "testnet"))]
pub const ASSET_SYMBOL: &str = "TEST";

pub const MAX_STR_LEN: usize = 26;
pub const MAX_PRECISION: u8 = 5;

#[derive(Copy, Clone, Default, PartialEq, PartialOrd)]
pub struct Asset {
    pub amount: i64,
}

impl Asset {
    #[inline]
    pub const fn new(amount: i64) -> Asset {
        Asset { amount }
    }

    #[inline]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Asset {
            amount: self.amount.checked_add(other.amount)?,
        })
    }

    #[inline]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        Some(Asset {
            amount: self.amount.checked_sub(other.amount)?,
        })
    }

    pub fn checked_mul(self, other: Self) -> Option<Self> {
        const MUL_PRECISION: u8 = MAX_PRECISION * 2;
        let mul = i128::from(self.amount).checked_mul(i128::from(other.amount))?;
        let final_mul = set_decimals_i128(mul, MUL_PRECISION, MAX_PRECISION)?;
        if final_mul > i128::from(::std::i64::MAX) {
            return None;
        }
        Some(Asset {
            amount: final_mul as i64,
        })
    }

    pub fn checked_div(self, other: Self) -> Option<Self> {
        if other.amount == 0 {
            return None;
        }
        const DIV_PRECISION: u8 = MAX_PRECISION * 2;
        let a = set_decimals_i64(self.amount, MAX_PRECISION, DIV_PRECISION)?;
        Some(Asset {
            amount: a.checked_div(other.amount)?,
        })
    }

    pub fn checked_pow(self, num: u16) -> Option<Self> {
        if num == 0 {
            return Some(Asset {
                amount: set_decimals_i64(1, 0, MAX_PRECISION)?,
            });
        }

        let decimals = u16::from(MAX_PRECISION).checked_mul(num)?;
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

        res = set_decimals_big(&res, decimals, u16::from(MAX_PRECISION));
        Some(Asset {
            amount: res.to_i64()?,
        })
    }
}

impl fmt::Debug for Asset {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Asset(\"{}\")", self.to_string())
    }
}

impl ToString for Asset {
    #[allow(clippy::comparison_chain)]
    fn to_string(&self) -> String {
        let mut s = self.amount.to_string();
        {
            let len = s.len();
            // See std::core::cmp for the most optimal comparisons
            if len < MAX_PRECISION as usize {
                let start = if self.amount < 0 { 1 } else { 0 };
                let diff = MAX_PRECISION as usize - len + start;
                s.insert_str(start, "0.");
                s.insert_str(start + 2, &"0".repeat(diff));
            } else if len == MAX_PRECISION as usize {
                s.insert_str(0, "0.");
            } else {
                s.insert(len - (MAX_PRECISION as usize), '.');
            }
        }
        s.push(' ');
        s.push_str(ASSET_SYMBOL);
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
        match split.next() {
            Some(x) => {
                match x.find('.') {
                    Some(pos) => {
                        // Check decimal precision is correct
                        {
                            let decimals = {
                                let len = x.len() - 1;
                                if pos > 0 {
                                    (len - pos) as u8
                                } else {
                                    len as u8
                                }
                            };

                            if decimals != MAX_PRECISION {
                                return Err(AssetError {
                                    kind: AssetErrorKind::InvalidFormat,
                                });
                            }
                        }

                        // Actually parse the amount
                        amount = match x.replace('.', "").parse() {
                            Ok(x) => x,
                            Err(_) => {
                                return Err(AssetError {
                                    kind: AssetErrorKind::InvalidAmount,
                                });
                            }
                        }
                    }
                    None => {
                        return Err(AssetError {
                            kind: AssetErrorKind::InvalidFormat,
                        });
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
                if x != ASSET_SYMBOL {
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

        Ok(Asset { amount })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_input() {
        let c = |asset: Asset, amount: &str| {
            assert_eq!(asset.amount.to_string(), amount);
        };

        c(get_asset("1.00000 TEST"), "100000");
        c(get_asset("-1.00000 TEST"), "-100000");
        c(get_asset(".10000 TEST"), "10000");
        c(get_asset("-.10000 TEST"), "-10000");
        c(get_asset("0.10000 TEST"), "10000");
        c(get_asset("0.00000 TEST"), "0");
        c(get_asset("-0.00000 TEST"), "0");
    }

    #[test]
    fn asset_to_str() {
        let c = |asset: Asset, s: &str| {
            assert_eq!(asset.to_string(), s);
        };
        c(get_asset("1.00001 TEST"), "1.00001 TEST");
        c(get_asset("0.00001 TEST"), "0.00001 TEST");
        c(get_asset("0.00010 TEST"), "0.00010 TEST");
        c(get_asset("-0.00001 TEST"), "-0.00001 TEST");
        c(get_asset(".00001 TEST"), "0.00001 TEST");
        c(get_asset(".10000 TEST"), "0.10000 TEST");
        c(get_asset("1.00000 TEST"), "1.00000 TEST");
    }

    #[test]
    fn fail_parsing_invalid_input() {
        let c = |asset: &str, err: AssetErrorKind| {
            let e = Asset::from_str(asset).err().unwrap();
            assert_eq!(e.kind, err, "Asset: {}", asset);
        };

        c("1b10.00000 TEST", AssetErrorKind::InvalidAmount);
        c("a100.00000 TEST", AssetErrorKind::InvalidAmount);
        c("100.0000a TEST", AssetErrorKind::InvalidAmount);

        c("1 TEST", AssetErrorKind::InvalidFormat);
        c("1. TEST", AssetErrorKind::InvalidFormat);
        c(".1 TEST", AssetErrorKind::InvalidFormat);
        c("-.1 TEST", AssetErrorKind::InvalidFormat);
        c("0.1 TEST", AssetErrorKind::InvalidFormat);
        c("1.0 TEST", AssetErrorKind::InvalidFormat);
        c("0 TEST", AssetErrorKind::InvalidFormat);
        c("-0.0 TEST", AssetErrorKind::InvalidFormat);
        c("-1.0 TEST", AssetErrorKind::InvalidFormat);

        c("1234567890123456789012 TEST", AssetErrorKind::StrTooLarge);
        c("1.000000 TEST", AssetErrorKind::InvalidFormat);
        c("1.0000", AssetErrorKind::InvalidFormat);

        c("1.00000 TEST a", AssetErrorKind::InvalidAssetType);
        c("1.00000 test", AssetErrorKind::InvalidAssetType);
    }

    #[test]
    fn perform_arithmetic() {
        let c = |asset: Asset, amount: &str| {
            assert_eq!(asset.to_string(), amount);
        };

        let a = get_asset("123.45600 TEST");
        c(
            a.checked_add(get_asset("2.00000 TEST")).unwrap(),
            "125.45600 TEST",
        );
        c(
            a.checked_add(get_asset("-2.00000 TEST")).unwrap(),
            "121.45600 TEST",
        );
        c(
            a.checked_add(get_asset(".00001 TEST")).unwrap(),
            "123.45601 TEST",
        );
        c(
            a.checked_sub(get_asset("2.00000 TEST")).unwrap(),
            "121.45600 TEST",
        );
        c(
            a.checked_sub(get_asset("-2.00000 TEST")).unwrap(),
            "125.45600 TEST",
        );
        c(
            a.checked_mul(get_asset("100000.11111 TEST")).unwrap(),
            "12345613.71719 TEST",
        );
        c(
            a.checked_mul(get_asset("-100000.11111 TEST")).unwrap(),
            "-12345613.71719 TEST",
        );
        c(
            a.checked_div(get_asset("23.00000 TEST")).unwrap(),
            "5.36765 TEST",
        );
        c(
            a.checked_div(get_asset("-23.00000 TEST")).unwrap(),
            "-5.36765 TEST",
        );
        c(a.checked_pow(2).unwrap(), "15241.38393 TEST");
        c(a.checked_pow(3).unwrap(), "1881640.29520 TEST");
        c(a, "123.45600 TEST");

        c(
            get_asset("1.00020 TEST").checked_pow(1000).unwrap(),
            "1.22137 TEST",
        );
        c(
            get_asset("10.00000 TEST")
                .checked_div(get_asset("2.00000 TEST"))
                .unwrap(),
            "5.00000 TEST",
        );
        c(
            get_asset("5.00000 TEST")
                .checked_div(get_asset("10.00000 TEST"))
                .unwrap(),
            "0.50000 TEST",
        );

        assert!(a.checked_div(get_asset("0.00000 TEST")).is_none());
    }

    #[test]
    fn invalid_arithmetic() {
        let a = get_asset("10.00000 TEST");
        let b = get_asset("92233720368547.75807 TEST");

        assert_eq!(a.checked_add(b), None);
        assert_eq!(a.checked_mul(Asset::new(-1)).unwrap().checked_sub(b), None);
        assert_eq!(a.checked_div(Asset::new(0)), None);
        assert_eq!(a.checked_mul(b), None);
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}
