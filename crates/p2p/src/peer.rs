use crate::*;
use network::connect::TcpConnect;
use tokio::{codec::FramedRead, prelude::*};

pub struct Peer {
    tx: Recipient<SessionMsg>,
}

impl Peer {
    pub fn init(conn: TcpConnect, tx: Recipient<SessionMsg>) {
        let stream = conn.0;
        let conn_type = conn.1;
        let peer_addr = stream.peer_addr().unwrap();
        Peer::create(move |ctx| {
            let peer_tx = ctx.address().recipient();
            debug!("[{}] Accepted {} connection", peer_addr, conn_type);
            Session::create(move |ctx| {
                let (r, w) = stream.split();
                ctx.add_stream(FramedRead::new(r, Codec::new()));
                Session {
                    recipient: peer_tx,
                    writer: actix::io::FramedWrite::new(w, Codec::new(), ctx),
                    info: SessionInfo {
                        id: peer_addr,
                        conn_type,
                        peer_addr,
                    },
                }
            });
            Peer { tx }
        });
    }
}

impl Actor for Peer {
    type Context = Context<Self>;
}

impl Handler<SessionMsg> for Peer {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, _: &mut Self::Context) {
        self.tx.do_send(msg).unwrap();
    }
}
