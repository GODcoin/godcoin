use tracing::{
    level_filters::LevelFilter,
    subscriber::{self, DefaultGuard},
};
use tracing_subscriber::EnvFilter;

pub fn init_tracing() -> DefaultGuard {
    let filter = EnvFilter::from_default_env().add_directive(LevelFilter::DEBUG.into());
    let sub = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .finish();
    subscriber::set_default(sub)
}
