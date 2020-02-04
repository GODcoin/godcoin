pub mod client;
pub mod minter;
pub mod pool;

use godcoin::{blockchain::ReindexOpts, prelude::*};
use log::{error, info, warn};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::TcpListener, prelude::*, time};

pub mod prelude {
    pub use super::minter::*;
    pub use super::pool::SubscriptionPool;
}

use prelude::*;

pub struct ServerOpts {
    pub blocklog_loc: PathBuf,
    pub index_loc: PathBuf,
    pub minter_key: KeyPair,
    pub bind_addr: String,
    pub reindex: Option<ReindexOpts>,
    pub enable_stale_production: bool,
}

#[derive(Clone)]
pub struct ServerData {
    pub chain: Arc<Blockchain>,
    pub minter: Minter,
    pub sub_pool: SubscriptionPool,
}

pub fn start(opts: ServerOpts) {
    let blockchain = Arc::new(Blockchain::new(&opts.blocklog_loc, &opts.index_loc));

    let is_empty = blockchain.is_empty();
    if !is_empty && blockchain.index_status() != IndexStatus::Complete {
        warn!(
            "Indexing not complete (status = {:?})",
            blockchain.index_status()
        );
        match opts.reindex {
            Some(opts) => blockchain.reindex(opts),
            None => panic!("index incomplete, aborting..."),
        }
    }

    if is_empty {
        let info = blockchain.create_genesis_block(opts.minter_key.clone());
        info!("=> Generated new block chain");
        info!("=> {:?}", info.script);
        for (index, key) in info.wallet_keys.iter().enumerate() {
            info!("=> Wallet key {}: {}", index + 1, key.1.to_wif());
        }
    }

    info!(
        "Using height in block log at {}",
        blockchain.get_chain_height()
    );

    let sub_pool = SubscriptionPool::new();
    let minter = Minter::new(
        Arc::clone(&blockchain),
        opts.minter_key,
        sub_pool.clone(),
        opts.enable_stale_production,
    );
    minter.clone().start_production_loop();

    let data = Arc::new(ServerData {
        chain: Arc::clone(&blockchain),
        minter,
        sub_pool,
    });

    let addr = opts.bind_addr.parse::<SocketAddr>().unwrap();
    start_server(addr, data);
}

fn start_server(server_addr: SocketAddr, data: Arc<ServerData>) {
    fn is_connection_error(e: &io::Error) -> bool {
        match e.kind() {
            io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset => true,
            _ => false,
        }
    }

    tokio::spawn(async move {
        let mut server = TcpListener::bind(&server_addr).await.unwrap();
        loop {
            match server.accept().await {
                Ok((stream, peer_addr)) => {
                    client::handle_new_client(stream, peer_addr, Arc::clone(&data));
                }
                Err(e) => {
                    error!("Accept error: {:?}", e);
                    match e {
                        ref e if is_connection_error(e) => continue,
                        _ => time::delay_for(Duration::from_millis(500)).await,
                    }
                }
            }
        }
    });
}
