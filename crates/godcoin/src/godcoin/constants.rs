use crate::asset::*;

pub const GRAEL_FEE_MIN: Asset = Asset::new(25);

pub const GRAEL_FEE_MULT: Asset = Asset::new(20_000);
pub const GRAEL_FEE_NET_MULT: Asset = Asset::new(10_150);

pub const NETWORK_FEE_AVG_WINDOW: u64 = 10;
pub const FEE_RESET_WINDOW: usize = 4;

pub const TX_EXPIRY_TIME: u64 = 30000;
pub const BLOCK_PROD_TIME: u64 = 3000;

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(GRAEL_FEE_MIN.to_string(), "0.0025 GRAEL");
        assert_eq!(GRAEL_FEE_MULT.to_string(), "2.0000 GRAEL");
        assert_eq!(GRAEL_FEE_NET_MULT.to_string(), "1.0150 GRAEL");
    }
}
