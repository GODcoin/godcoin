#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TxType {
    OWNER = 0,
    MINT = 1,
    REWARD = 2,
    TRANSFER = 3,
}
