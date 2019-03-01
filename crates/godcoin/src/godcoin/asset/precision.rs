#![allow(clippy::unreadable_literal)]

use num_bigint::BigInt;

const DECIMAL_MULT_I64: [i64; 19] = [
    1,
    10,
    100,
    1000,
    10000,
    100000,
    1000000,
    10000000,
    100000000,
    1000000000,
    10000000000,
    100000000000,
    1000000000000,
    10000000000000,
    100000000000000,
    1000000000000000,
    10000000000000000,
    100000000000000000,
    1000000000000000000,
];

const DECIMAL_MULT_I128: [i128; 39] = [
    1,
    10,
    100,
    1000,
    10000,
    100000,
    1000000,
    10000000,
    100000000,
    1000000000,
    10000000000,
    100000000000,
    1000000000000,
    10000000000000,
    100000000000000,
    1000000000000000,
    10000000000000000,
    100000000000000000,
    1000000000000000000,
    10000000000000000000,
    100000000000000000000,
    1000000000000000000000,
    10000000000000000000000,
    100000000000000000000000,
    1000000000000000000000000,
    10000000000000000000000000,
    100000000000000000000000000,
    1000000000000000000000000000,
    10000000000000000000000000000,
    100000000000000000000000000000,
    1000000000000000000000000000000,
    10000000000000000000000000000000,
    100000000000000000000000000000000,
    1000000000000000000000000000000000,
    10000000000000000000000000000000000,
    100000000000000000000000000000000000,
    1000000000000000000000000000000000000,
    10000000000000000000000000000000000000,
    100000000000000000000000000000000000000,
];

macro_rules! create_decimal_fn {
    ($ty:ty, $set_decimals:ident, $DECIMAL_MULT:ident) => {
        pub fn $set_decimals(num: $ty, orig_dec: u8, new_dec: u8) -> Option<$ty> {
            if orig_dec > new_dec {
                num.checked_div($DECIMAL_MULT[(orig_dec - new_dec) as usize])
            } else if orig_dec < new_dec {
                num.checked_mul($DECIMAL_MULT[(new_dec - orig_dec) as usize])
            } else {
                Some(num)
            }
        }
    };
}

create_decimal_fn!(i64, set_decimals_i64, DECIMAL_MULT_I64);
create_decimal_fn!(i128, set_decimals_i128, DECIMAL_MULT_I128);

pub fn set_decimals_big(num: &BigInt, orig_dec: u16, new_dec: u16) -> BigInt {
    if orig_dec > new_dec {
        let delta = (orig_dec - new_dec) as usize;
        if delta < DECIMAL_MULT_I64.len() {
            num / DECIMAL_MULT_I64[delta]
        } else {
            let mut rep = String::from("1");
            rep.push_str(&"0".repeat(delta));
            num / rep.parse::<BigInt>().unwrap()
        }
    } else if orig_dec < new_dec {
        let delta = (new_dec - orig_dec) as usize;
        if delta < DECIMAL_MULT_I64.len() {
            num * DECIMAL_MULT_I64[delta]
        } else {
            let mut rep = String::from("1");
            rep.push_str(&"0".repeat(delta));
            num * rep.parse::<BigInt>().unwrap()
        }
    } else {
        num.clone()
    }
}
