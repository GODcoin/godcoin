use actix::actors::signal;
use actix::prelude::*;
use godcoin_p2p::{session::*, NetCmd, Network, Payload};
use log::info;
use std::{net::SocketAddr, time::Duration};

struct Signals;

impl Actor for Signals {
    type Context = Context<Self>;
}

impl Handler<signal::Signal> for Signals {
    type Result = ();

    fn handle(&mut self, msg: signal::Signal, _: &mut Context<Self>) {
        if let signal::SignalType::Int = msg.0 {
            info!("SIGINT received, exiting");
            System::current().stop();
        }
    }
}

struct DisconnectTimer<S: 'static> {
    dur: Duration,
    node_addr: SocketAddr,
    addr: Addr<Network<S>>,
}

impl<S: 'static> Actor for DisconnectTimer<S> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        ctx.run_later(self.dur, |act, _| {
            act.addr.do_send(NetCmd::Disconnect(act.node_addr));
        });
    }
}

struct NetInfo {
    net_id: usize,
}

fn connected(state: &mut NetInfo, ses: SessionInfo) {
    match ses.conn_type {
        ConnectionType::Inbound => {
            info!(
                "[net:{}] Accepted connection -> {}",
                state.net_id, ses.peer_addr
            );
        }
        ConnectionType::Outbound => {
            info!(
                "[net:{}] Connected to node -> {}",
                state.net_id, ses.peer_addr
            );
        }
    }
}

fn disconnected(state: &mut NetInfo, ses: SessionInfo) {
    info!(
        "[net:{}] Connection disconnected -> {}",
        state.net_id, ses.peer_addr
    );
}

fn message(state: &mut NetInfo, id: SessionId, payload: Payload) {
    info!(
        "[net:{}] Received message from {} with: {:?}",
        state.net_id, id, payload
    );
}

fn main() {
    let env = env_logger::Env::new()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, "godcoin_simulation=debug");
    env_logger::init_from_env(env);

    let sys = System::new("simulation");

    {
        let sig_addr = Signals.start();
        let addr = signal::ProcessSignals::from_registry();
        addr.do_send(signal::Subscribe(sig_addr.recipient()));
    }
    {
        let net = Network::new(NetInfo { net_id: 0 }, message)
            .on_connect(connected)
            .on_disconnect(disconnected)
            .start();
        net.do_send(NetCmd::Listen("127.0.0.1:7777".parse().unwrap()));
        info!("[net:{}] Accepting connections on 127.0.0.1:7777", 0);
    }
    {
        let net = Network::new(NetInfo { net_id: 1 }, message)
            .on_connect(connected)
            .on_disconnect(disconnected)
            .start();
        let node_addr = "127.0.0.1:7777".parse().unwrap();
        net.do_send(NetCmd::Connect(node_addr));

        DisconnectTimer {
            dur: Duration::from_secs(5),
            node_addr,
            addr: net,
        }
        .start();
    }

    sys.run();
}
