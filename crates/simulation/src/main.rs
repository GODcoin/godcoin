use actix::actors::signal;
use actix::prelude::*;
use godcoin_p2p::{msg, session, NetCmd, Network};
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

struct DisconnectTimer {
    dur: Duration,
    node_addr: SocketAddr,
    addr: Addr<Network>,
}

impl Actor for DisconnectTimer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        ctx.run_later(self.dur, |act, _| {
            act.addr.do_send(NetCmd::Disconnect(act.node_addr));
        });
    }
}

struct MsgHandler {
    net_id: usize,
}

impl Actor for MsgHandler {
    type Context = Context<Self>;
}

impl Handler<msg::Connected> for MsgHandler {
    type Result = ();

    fn handle(&mut self, msg: msg::Connected, _: &mut Self::Context) {
        let msg::Connected(ses) = msg;
        match ses.conn_type {
            session::ConnectionType::Inbound => {
                info!(
                    "[net:{}] Accepted connection -> {}",
                    self.net_id, ses.peer_addr
                );
            }
            session::ConnectionType::Outbound => {
                info!(
                    "[net:{}] Connected to node -> {}",
                    self.net_id, ses.peer_addr
                );
            }
        }
    }
}

impl Handler<msg::Disconnected> for MsgHandler {
    type Result = ();

    fn handle(&mut self, msg: msg::Disconnected, _: &mut Self::Context) {
        let msg::Disconnected(ses) = msg;
        info!(
            "[net:{}] Connection disconnected -> {}",
            self.net_id, ses.peer_addr
        );
    }
}

impl Handler<msg::Message> for MsgHandler {
    type Result = ();

    fn handle(&mut self, msg: msg::Message, _: &mut Self::Context) {
        let msg::Message(ses_id, payload) = msg;
        info!(
            "[net:{}] Received message from {} with: {:?}",
            self.net_id, ses_id, payload
        );
    }
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
        let handler = MsgHandler { net_id: 0 }.start();
        let mut net = Network::new(&handler);
        net.subscribe_connect(&handler);
        net.subscribe_disconnect(&handler);
        let addr = net.start();
        addr.do_send(NetCmd::Listen("127.0.0.1:7777".parse().unwrap()));
        info!("[net:{}] Accepting connections on 127.0.0.1:7777", 0);
    }
    {
        let handler = MsgHandler { net_id: 1 }.start();
        let mut net = Network::new(&handler);
        net.subscribe_connect(&handler);
        net.subscribe_disconnect(&handler);
        let addr = net.start();
        let node_addr = "127.0.0.1:7777".parse().unwrap();
        addr.do_send(NetCmd::Connect(node_addr));

        DisconnectTimer {
            dur: Duration::from_secs(5),
            node_addr,
            addr,
        }
        .start();
    }

    sys.run();
}
