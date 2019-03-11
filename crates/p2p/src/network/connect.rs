use super::*;
use tokio::net::TcpStream;

#[derive(Message)]
pub struct TcpConnect(pub TcpStream, pub session::ConnectionType);

impl<S: 'static, M: 'static + Metrics> Handler<TcpConnect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, conn: TcpConnect, ctx: &mut Self::Context) {
        Peer::init(conn, ctx.address().recipient());
    }
}
