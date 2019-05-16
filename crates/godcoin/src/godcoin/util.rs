use std::time::{SystemTime, UNIX_EPOCH};

macro_rules! u64_from_buf {
    ($buf:expr, $offset:expr) => {
        (u64::from($buf[$offset]) << 56)
            | (u64::from($buf[$offset + 1]) << 48)
            | (u64::from($buf[$offset + 2]) << 40)
            | (u64::from($buf[$offset + 3]) << 32)
            | (u64::from($buf[$offset + 4]) << 24)
            | (u64::from($buf[$offset + 5]) << 16)
            | (u64::from($buf[$offset + 6]) << 8)
            | u64::from($buf[$offset + 7])
    };
    ($buf:expr) => {
        u64_from_buf!($buf, 0)
    };
}

macro_rules! u32_from_buf {
    ($buf:expr, $offset:expr) => {
        (u32::from($buf[$offset]) << 24)
            | (u32::from($buf[$offset + 1]) << 16)
            | (u32::from($buf[$offset + 2]) << 8)
            | u32::from($buf[$offset + 3])
    };
    ($buf:expr) => {
        u32_from_buf!($buf, 0)
    };
}

pub fn get_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
