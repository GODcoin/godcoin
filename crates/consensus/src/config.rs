use crate::NodeId;
use rand::Rng;

#[derive(Clone, Debug)]
pub struct Config {
    pub id: NodeId,
    pub heartbeat_timeout: u32,
    pub min_election_timeout: u32,
    pub max_election_timeout: u32,
}

impl Config {
    /// Creates a default configuration with the specified ID and the assumption that every 100ms is
    /// a tick.
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            heartbeat_timeout: 2,
            min_election_timeout: 10,
            max_election_timeout: 20,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id == 0 {
            return Err("ID cannot be 0");
        }

        if self.min_election_timeout == 0 {
            return Err("Minimum election timeout cannot be 0");
        }

        if self.min_election_timeout <= self.heartbeat_timeout {
            return Err("Minimum election timeout must be greater than the heartbeat timeout");
        }

        if self.min_election_timeout >= self.max_election_timeout {
            return Err("Maximum election timeout must be greater than the minimum timeout");
        }

        Ok(())
    }

    pub fn random_election_timeout(&self) -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen_range(self.min_election_timeout, self.max_election_timeout)
    }
}
