pub type SkipFlags = u8;

#[allow(clippy::identity_op)]
pub const SKIP_NONE: u8 = 1 << 0;
pub const SKIP_REWARD_TX: u8 = 1 << 1;
