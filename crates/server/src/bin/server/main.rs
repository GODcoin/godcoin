use clap::{App, Arg};
use godcoin::{blockchain::ReindexOpts, prelude::*};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Response, Server, StatusCode,
};
use prometheus::{Encoder, TextEncoder};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tokio::runtime::Builder;
use tracing::{error, info};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

#[derive(Debug, Deserialize)]
struct Config {
    minter_key: String,
    enable_stale_production: bool,
    bind_address: Option<String>,
    metrics_bind_address: Option<String>,
}

fn main() {
    install_panic_hook();

    let filter = EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into());
    tracing_subscriber::fmt().with_env_filter(filter).init();

    godcoin::init().unwrap();
    godcoin_server::init();

    let mut rt = Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap();

    rt.spawn(async move {
        let home = {
            match env::var("GODCOIN_HOME") {
                Ok(s) => PathBuf::from(s),
                Err(_) => Path::join(&dirs::data_local_dir().unwrap(), "godcoin"),
            }
        };

        let home = home.to_string_lossy();
        let args = App::new("godcoin-server")
            .about("GODcoin core server daemon")
            .version(env!("CARGO_PKG_VERSION"))
            .arg(
                Arg::with_name("home")
                    .long("home")
                    .default_value(&home)
                    .empty_values(false)
                    .help("Home directory which defaults to env var GODCOIN_HOME"),
            )
            .arg(
                Arg::with_name("reindex")
                    .long("reindex")
                    .help("Reindexes the block log"),
            )
            .arg(
                Arg::with_name("auto_trim")
                    .long("reindex-trim-corrupt")
                    .help("Trims any corruption detected in the block log during reindexing"),
            )
            .get_matches();

        let home = PathBuf::from(args.value_of("home").expect("Failed to obtain home path"));
        let (blocklog_loc, index_loc) = {
            if !Path::is_dir(&home) {
                let res = std::fs::create_dir(&home);
                res.unwrap_or_else(|_| panic!("Failed to create dir at {:?}", &home));
                info!("Created GODcoin home at {:?}", &home);
            } else {
                info!("Found GODcoin home at {:?}", &home);
            }
            let blocklog_loc = Path::join(&home, "blklog");
            let index_loc = Path::join(&home, "index");
            (blocklog_loc, index_loc)
        };

        let config_file = Path::join(&home, "config.toml");
        info!("Opening configuration file at {:?}", config_file);
        let config_file = fs::read(config_file).expect("Failed to open config");
        let config: Config = toml::from_str(&String::from_utf8(config_file).unwrap()).unwrap();

        if let Some(bind_address) = config.metrics_bind_address {
            let service = make_service_fn(|_| async {
                Ok::<_, hyper::Error>(service_fn(move |_req| async {
                    let encoder = TextEncoder::new();
                    let metrics = prometheus::gather();

                    let mut buf = vec![];
                    match encoder.encode(&metrics, &mut buf) {
                        Ok(()) => Ok::<_, hyper::Error>(Response::new(Body::from(buf))),
                        Err(e) => {
                            error!("Error encoding metrics: {:?}", e);
                            let mut res = Response::default();
                            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Ok(res)
                        }
                    }
                }))
            });

            info!("Metrics monitoring server is starting on {}", bind_address);
            let addr: std::net::SocketAddr = bind_address.parse().unwrap();
            let server = Server::bind(&addr).serve(service);

            info!("Metrics server listening on http://{}", addr);
            tokio::spawn(server);
        } else {
            info!("Metrics monitoring is disabled");
        }

        let minter_key =
            PrivateKey::from_wif(&config.minter_key).expect("Provided minter key is invalid");
        let bind_addr = config
            .bind_address
            .unwrap_or_else(|| "127.0.0.1:7777".to_string());

        let reindex = if args.is_present("reindex") {
            info!("User requested reindexing");
            if Path::exists(&index_loc) {
                info!("Deleting current index");
                fs::remove_dir_all(&index_loc)
                    .expect("Failed to delete the blockchain index directory");
            } else {
                info!("Current index does not exist");
            }
            let auto_trim = args.is_present("auto_trim");
            Some(ReindexOpts { auto_trim })
        } else {
            None
        };

        let enable_stale_production = config.enable_stale_production;
        godcoin_server::start(godcoin_server::ServerOpts {
            blocklog_loc,
            index_loc,
            minter_key,
            bind_addr,
            reindex,
            enable_stale_production,
        });
    });

    rt.block_on(async {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Received ctrl-c, shutting down...");
    });
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);
        std::process::abort();
    }));
}
