use crate::asset::*;

pub const GOLD_FEE_MIN: Asset = Asset {
    amount: 100,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD
};

pub const SILVER_FEE_MIN: Asset = Asset {
    amount: 1000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::SILVER
};

pub const GOLD_FEE_MULT: Asset = Asset {
    amount: 200_000_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD
};

pub const SILVER_FEE_MULT: Asset = Asset {
    amount: 200_000_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::SILVER
};

pub const GOLD_FEE_NET_MULT: Asset = Asset {
    amount: 100_200_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD
};

pub const SILVER_FEE_NET_MULT: Asset = Asset {
    amount: 100_200_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::SILVER
};

pub const BOND_FEE: Asset = Asset {
    amount: 500_000_000,
    decimals: MAX_PRECISION,
    symbol: AssetSymbol::GOLD
};

pub const NETWORK_FEE_AVG_WINDOW: u64 = 10;
pub const FEE_RESET_WINDOW: usize = 4;

pub const BLOCK_PROD_TIME: u64 = 3000;

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(GOLD_FEE_MIN.to_string(), "0.00000100 GOLD");
        assert_eq!(SILVER_FEE_MIN.to_string(), "0.00001000 SILVER");

        assert_eq!(GOLD_FEE_MULT.to_string(), "2.00000000 GOLD");
        assert_eq!(SILVER_FEE_MULT.to_string(), "2.00000000 SILVER");

        assert_eq!(GOLD_FEE_NET_MULT.to_string(), "1.00200000 GOLD");
        assert_eq!(SILVER_FEE_NET_MULT.to_string(), "1.00200000 SILVER");

        assert_eq!(BOND_FEE.to_string(), "5.00000000 GOLD");
    }
}
