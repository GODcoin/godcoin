#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TxType {
    OWNER = 0,
    MINT = 1,
    REWARD = 2,
    TRANSFER = 3,
}
