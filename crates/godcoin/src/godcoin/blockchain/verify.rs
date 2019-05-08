#[derive(Copy, Clone, Debug)]
pub struct Config {
    pub skip_reward: bool,
}

impl Config {
    pub const fn strict() -> Self {
        Self { skip_reward: false }
    }
}
