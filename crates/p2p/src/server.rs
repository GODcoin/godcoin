use crate::{network::connect::*, *};
use std::io::Error;
use tokio::net::TcpStream;

pub struct Server {
    pub recipient: Recipient<TcpConnect>,
}

impl Actor for Server {
    type Context = Context<Self>;
}

impl StreamHandler<TcpStream, Error> for Server {
    fn handle(&mut self, s: TcpStream, _: &mut Self::Context) {
        self.recipient.do_send(TcpConnect(s, None)).unwrap();
    }

    fn error(&mut self, err: Error, _: &mut Self::Context) -> Running {
        error!("Failed to accept incoming connection: {:?}", err);
        Running::Continue
    }
}
