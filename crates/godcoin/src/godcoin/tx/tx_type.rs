#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TxType {
    REWARD = 0,
    TRANSFER = 1,
    BOND = 2,
}
