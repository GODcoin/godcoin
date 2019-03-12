use super::*;
use tokio::net::TcpStream;
use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum ConnectionType {
    Inbound,
    Outbound,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectionType::Inbound => f.write_str("inbound"),
            ConnectionType::Outbound => f.write_str("outbound"),
        }
    }
}

#[derive(Message)]
pub struct TcpConnect(pub TcpStream, pub connect::ConnectionType);

impl<S: 'static, M: 'static + Metrics> Handler<TcpConnect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, conn: TcpConnect, ctx: &mut Self::Context) {
        let addr = ctx.address();
        Peer::init(conn, addr);
    }
}
