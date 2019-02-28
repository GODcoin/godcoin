use godcoin_p2p::{Network, NetCmd};
use actix::actors::signal;
use actix::prelude::*;
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

fn main() {
    let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "godcoin=debug");
    env_logger::init_from_env(env);

    let sys = System::new("simulation");

    {
        let sig_addr = Signals.start();
        let addr = signal::ProcessSignals::from_registry();
        addr.do_send(signal::Subscribe(sig_addr.recipient()));
    }
    {
        let net = Network::new();
        let addr = net.start();
        addr.do_send(NetCmd::Listen("127.0.0.1:7777".parse().unwrap()));
    }
    {
        let net = Network::new();
        let addr = net.start();
        addr.do_send(NetCmd::Connect("127.0.0.1:7777".parse().unwrap()));
    }

    sys.run();
}
