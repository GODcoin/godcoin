use actix::prelude::*;
use bytes::BytesMut;
use futures::future::join_all;
use godcoin_p2p::*;
use log::{error, info};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::{prelude::*, timer::Delay};

mod handlers;
mod signals;

fn main() {
    godcoin_p2p::init();

    let env = env_logger::Env::new()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, "godcoin_simulation=debug");
    env_logger::init_from_env(env);

    let sys = System::new("simulation");
    signals::Signals::init();

    let nets = {
        let net_count = 3;
        let port = 7777;
        let mut nets = Vec::with_capacity(net_count);
        let msg_counter = Rc::new(RefCell::new(0));
        // The threshold is always one less to exclude the current network from being counted
        let threshold = net_count - 1;
        for net_id in 0..net_count {
            let msg_counter = Rc::clone(&msg_counter);
            let state = handlers::NetState::new(net_id, msg_counter, threshold);
            let net = Network::new(state, handlers::message)
                .with_metrics(BasicMetrics::default())
                .on_connect_req(handlers::connect_req)
                .on_connect(handlers::connected)
                .on_disconnect(handlers::disconnected)
                .start();

            let port = port + net_id;
            net.do_send(cmd::Listen(format!("127.0.0.1:{}", port).parse().unwrap()));
            info!(
                "[net:{}] Accepting connections on 127.0.0.1:{}",
                net_id, port
            );
            nets.push(net);
        }
        nets
    };

    nets[1].do_send(cmd::Connect("127.0.0.1:7777".parse().unwrap()));
    nets[2].do_send(cmd::Connect("127.0.0.1:7777".parse().unwrap()));
    nets[2].do_send(cmd::Connect("127.0.0.1:7778".parse().unwrap()));

    let deadline = Instant::now() + Duration::from_secs(1);
    Arbiter::spawn(
        Delay::new(deadline)
            .and_then({
                let nets = nets.clone();
                move |_| {
                    let payload = Payload {
                        id: BytesMut::from(vec![1, 2, 3]),
                        msg: BytesMut::from(vec![4, 5, 6]),
                    };
                    println!();
                    info!("[net:2] Broadcasting message: {:?}", &payload);
                    nets[2].do_send(cmd::Broadcast(payload));
                    Ok(())
                }
            })
            .map_err(|e| {
                error!("Timer failed: {:?}", e);
            }),
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    Arbiter::spawn(
        Delay::new(deadline)
            .and_then(move |_| {
                println!();
                let futs = nets
                    .iter()
                    .map(|net| Box::new(net.clone().send(cmd::Metrics::default())))
                    .collect::<Vec<Box<_>>>();

                Arbiter::spawn(
                    join_all(futs)
                        .and_then(|res| {
                            for (net_id, metrics) in res.iter().enumerate() {
                                info!("[net:{}] {:?}", net_id, metrics);
                            }
                            Ok(())
                        })
                        .map_err(|e| {
                            println!("ERROR: {:?}", e);
                        }),
                );
                Ok(())
            })
            .map_err(|e| {
                error!("Timer failed: {:?}", e);
            }),
    );

    sys.run();
}
