use godcoin::{*, net_v1::PeerPool, producer::Minter};
use std::borrow::Cow;
use std::sync::Arc;
use log::info;

pub struct Node<'a> {
    pub bind_address: Option<&'a str>,
    pub minter_key: Option<KeyPair>,
    pub peers: Option<Vec<&'a str>>
}

impl<'a> Node<'a> {
    pub fn start(&self) {
        use godcoin::blockchain::*;
        use std::{env, path::*};

        let home: PathBuf = {
            use dirs;
            let home = env::var("GODCOIN_HOME").map(|s| {
                PathBuf::from(s)
            }).unwrap_or_else(|_| {
                Path::join(&dirs::data_local_dir().unwrap(), "godcoin")
            });
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
        info!("Using height in block log at {}", blockchain.get_chain_height());

        let blockchain = Arc::new(blockchain);
        let minter = match &self.minter_key {
            Some(key) => {
                let bond = blockchain.get_bond(&key.0).expect("No bond found for minter key");
                let minter = key.clone();
                let staker = bond.staker;
                let minter = Minter::new(Arc::clone(&blockchain), minter, staker);
                Arc::new(Some(minter))
            },
            None => Arc::new(None)
        };

        {
            let peers = if let Some(peers) = &self.peers {
                Cow::Borrowed(peers)
            } else {
                Cow::Owned(vec![])
            };
            let pool = PeerPool::new(&peers);
            pool.start(&blockchain, &minter);

            // TODO synchronize blocks with peers
        }

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

        if let Some(bind) = self.bind_address {
            let addr = bind.parse()
                            .unwrap_or_else(|_| panic!("Failed to parse address: {:?}", bind));
            net_v1::server::start(addr, blockchain, minter);
        }
    }
}
