use actix::actors::signal;
use actix::prelude::*;
use godcoin_p2p::{session, NetCmd, NetMsg, Network};
use log::info;

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

struct MsgHandler {
    net_id: usize,
}

impl Actor for MsgHandler {
    type Context = Context<Self>;
}

impl Handler<NetMsg> for MsgHandler {
    type Result = ();

    fn handle(&mut self, msg: NetMsg, _: &mut Self::Context) {
        match msg {
            NetMsg::Connected(msg) => match msg.conn_type {
                session::ConnectionType::Inbound => {
                    info!("[net:{}] Accepted connection -> {}", self.net_id, msg.addr);
                }
                session::ConnectionType::Outbound => {
                    info!("[net:{}] Connected to node -> {}", self.net_id, msg.addr);
                }
            },
            NetMsg::Disconnected(msg) => {
                info!(
                    "[net:{}] Connection disconnected -> {}",
                    self.net_id, msg.addr
                );
            }
            NetMsg::Message(ses_id, payload) => {
                info!(
                    "[net:{}] Received message from {} with: {:?}",
                    self.net_id, ses_id, payload
                );
            }
        }
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
        let net = Network::new(handler.recipient());
        let addr = net.start();
        addr.do_send(NetCmd::Listen("127.0.0.1:7777".parse().unwrap()));
        info!("[net:{}] Accepting connections on 127.0.0.1:7777", 0);
    }
    {
        let handler = MsgHandler { net_id: 1 }.start();
        let net = Network::new(handler.recipient());
        let addr = net.start();
        addr.do_send(NetCmd::Connect("127.0.0.1:7777".parse().unwrap()));
    }

    sys.run();
}
