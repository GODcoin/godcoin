use godcoin::{producer::Minter, *};
use log::info;
use std::sync::Arc;

pub struct Node<'a> {
    pub bind_address: Option<&'a str>,
    pub minter_key: Option<KeyPair>,
    pub peers: Option<Vec<&'a str>>,
}

impl<'a> Node<'a> {
    pub fn start(&self) {
        use godcoin::blockchain::*;
        use std::{env, path::*};

        let home: PathBuf = {
            let home = {
                use dirs;
                match env::var("GODCOIN_HOME") {
                    Ok(s) => PathBuf::from(s),
                    Err(_) => Path::join(&dirs::data_local_dir().unwrap(), "godcoin"),
                }
            };
            if !Path::is_dir(&home) {
                let res = std::fs::create_dir(&home);
                res.unwrap_or_else(|_| panic!("Failed to create dir at {:?}", &home));
                info!("Created GODcoin home at {:?}", &home);
            } else {
                info!("Found GODcoin home at {:?}", &home);
            }
            home
        };

        let blockchain = Blockchain::new(&home);
        info!(
            "Using height in block log at {}",
            blockchain.get_chain_height()
        );

        let blockchain = Arc::new(blockchain);
        let minter = match &self.minter_key {
            Some(key) => {
                let bond = blockchain
                    .get_bond(&key.0)
                    .expect("No bond found for minter key");
                let minter = key.clone();
                let staker = bond.staker;
                let minter = Minter::new(Arc::clone(&blockchain), minter, staker);
                Arc::new(Some(minter))
            }
            None => Arc::new(None),
        };

        {
            let create_genesis = blockchain.get_block(0).is_none();
            if create_genesis && self.minter_key.is_some() {
                if let Some(ref key) = self.minter_key {
                    blockchain.create_genesis_block(key);
                }
            }
        }

        if let Some(minter) = minter.as_ref() {
            minter.clone().start_timer();
        }
    }
}
