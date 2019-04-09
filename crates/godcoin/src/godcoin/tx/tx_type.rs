#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TxType {
    OWNER = 0,
    REWARD = 1,
    TRANSFER = 2,
}
