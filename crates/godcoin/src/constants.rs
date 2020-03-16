use crate::asset::*;

pub const GRAEL_FEE_MIN: Asset = Asset::new(25);

pub const GRAEL_FEE_MULT: Asset = Asset::new(200_000);
pub const GRAEL_FEE_NET_MULT: Asset = Asset::new(101_500);

pub const GRAEL_ACC_CREATE_FEE_MULT: Asset = Asset::new(200_000);
pub const GRAEL_ACC_CREATE_MIN_BAL_MULT: Asset = Asset::new(200_000);

pub const NETWORK_FEE_AVG_WINDOW: u64 = 10;
pub const FEE_RESET_WINDOW: usize = 4;

pub const TX_MAX_EXPIRY_TIME: u64 = 60 * 60 * 24 * 30;
pub const BLOCK_PROD_TIME: u64 = 3;

pub const MAX_MEMO_BYTE_SIZE: usize = 1024;
pub const MAX_SCRIPT_BYTE_SIZE: usize = 2048;
pub const MAX_TX_SIGNATURES: usize = 8;

#[cfg(not(any(test, feature = "testnet")))]
pub const CHAIN_ID: [u8; 2] = [0x00, 0x00];

#[cfg(any(test, feature = "testnet"))]
pub const CHAIN_ID: [u8; 2] = [0x00, 0x01];

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(GRAEL_FEE_MIN.to_string(), "0.00025 TEST");
        assert_eq!(GRAEL_FEE_MULT.to_string(), "2.00000 TEST");
        assert_eq!(GRAEL_FEE_NET_MULT.to_string(), "1.01500 TEST");

        assert_eq!(GRAEL_ACC_CREATE_FEE_MULT.to_string(), "2.00000 TEST");
        assert_eq!(GRAEL_ACC_CREATE_MIN_BAL_MULT.to_string(), "2.00000 TEST");

        // Test that we are running in testnet mode
        assert_eq!(CHAIN_ID, [0x00, 0x01]);
    }
}
