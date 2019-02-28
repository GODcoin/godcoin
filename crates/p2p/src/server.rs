use tokio::net::TcpStream;
use std::io::Error;
use crate::*;

pub struct Server {
    pub recipient: Recipient<SessionMsg>
}

impl Actor for Server {
    type Context = Context<Self>;
}

impl StreamHandler<TcpStream, Error> for Server {
    fn handle(&mut self, s: TcpStream, _: &mut Self::Context) {
        let recipient = self.recipient.clone();
        Session::init(recipient, ConnectionType::Inbound, s);
    }

    fn error(&mut self, err: Error, _: &mut Self::Context) -> Running {
        error!("Failed to accept incoming connection: {:?}", err);
        Running::Continue
    }
}
