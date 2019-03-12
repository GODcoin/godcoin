use super::*;
use std::net::SocketAddr;
use tokio::net::TcpStream;

#[derive(Message)]
pub struct TcpConnect(pub TcpStream, pub Option<SocketAddr>);

impl<S: 'static, M: 'static + Metrics> Handler<TcpConnect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, conn: TcpConnect, ctx: &mut Self::Context) {
        let addr = ctx.address();
        Peer::init(conn, addr);
    }
}
