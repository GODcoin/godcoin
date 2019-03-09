use super::*;
use tokio::{codec::FramedRead, net::TcpStream, prelude::*};

#[derive(Message)]
pub struct TcpConnect(pub TcpStream, pub session::ConnectionType);

impl<S: 'static, M: 'static + Metrics> Handler<TcpConnect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, conn: TcpConnect, ctx: &mut Self::Context) {
        let stream = conn.0;
        let conn_type = conn.1;
        let tx = ctx.address().recipient();
        let peer_addr = stream.peer_addr().unwrap();
        // TODO: perform the handshake
        debug!("[{}] Accepted {} connection", peer_addr, conn_type);
        Session::create(move |ctx| {
            let (r, w) = stream.split();
            ctx.add_stream(FramedRead::new(r, Codec::new()));
            Session {
                recipient: tx,
                writer: actix::io::FramedWrite::new(w, Codec::new(), ctx),
                info: SessionInfo {
                    id: peer_addr,
                    conn_type,
                    peer_addr,
                },
            }
        });
    }
}
