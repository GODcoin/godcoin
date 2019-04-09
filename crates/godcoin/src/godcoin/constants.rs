use crate::asset::*;

pub const GOLD_FEE_MIN: Asset = Asset {
    amount: 25,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD,
};

pub const SILVER_FEE_MIN: Asset = Asset {
    amount: 205,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::SILVER,
};

pub const GOLD_FEE_MULT: Asset = Asset {
    amount: 20_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD,
};

pub const SILVER_FEE_MULT: Asset = Asset {
    amount: 20_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::SILVER,
};

pub const GOLD_FEE_NET_MULT: Asset = Asset {
    amount: 10_150,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD,
};

pub const SILVER_FEE_NET_MULT: Asset = Asset {
    amount: 10_150,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::SILVER,
};

pub const NETWORK_FEE_AVG_WINDOW: u64 = 10;
pub const FEE_RESET_WINDOW: usize = 4;

pub const BLOCK_PROD_TIME: u64 = 3000;

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(GOLD_FEE_MIN.to_string(), "0.0025 GOLD");
        assert_eq!(SILVER_FEE_MIN.to_string(), "0.0205 SILVER");

        assert_eq!(GOLD_FEE_MULT.to_string(), "2.0000 GOLD");
        assert_eq!(SILVER_FEE_MULT.to_string(), "2.0000 SILVER");

        assert_eq!(GOLD_FEE_NET_MULT.to_string(), "1.0150 GOLD");
        assert_eq!(SILVER_FEE_NET_MULT.to_string(), "1.0150 SILVER");
    }
}
