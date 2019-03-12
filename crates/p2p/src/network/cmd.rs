use super::{connect::*, *};
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    prelude::*,
};

#[derive(Message)]
pub struct Listen(pub SocketAddr);

#[derive(Message)]
pub struct Connect(pub SocketAddr);

#[derive(Message)]
pub struct Disconnect(pub PeerId);

#[derive(Message)]
pub struct Broadcast(pub Payload);

#[derive(Default, Message)]
#[rtype(result = "M")]
pub struct Metrics<M: 'static + crate::Metrics> {
    _metrics: std::marker::PhantomData<M>,
}

impl<S: 'static, M: 'static + crate::Metrics> Handler<cmd::Listen> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Listen, ctx: &mut Self::Context) {
        let bind_addr = msg.0;
        let recipient = ctx.address().recipient();
        Server::create(move |ctx| {
            let s = TcpListener::bind(&bind_addr).unwrap();
            debug!("Accepting inbound connections on {}", bind_addr);
            ctx.add_stream(s.incoming());
            Server { recipient }
        });
    }
}

impl<S: 'static, M: 'static + crate::Metrics> Handler<cmd::Connect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Connect, ctx: &mut Self::Context) {
        let peer_addr = msg.0;
        let addr = ctx.address();
        Arbiter::spawn(
            TcpStream::connect(&peer_addr)
                .and_then(move |s| {
                    addr.do_send(TcpConnect(s, Some(peer_addr)));
                    Ok(())
                })
                .map_err(move |e| {
                    warn!("[{}] Failed to connect to peer: {:?}", peer_addr, e);
                }),
        );
    }
}

impl<S: 'static, M: 'static + crate::Metrics> Handler<cmd::Disconnect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Disconnect, _: &mut Self::Context) {
        if let Some(ses) = self.sessions.get(&msg.0) {
            ses.1.do_send(crate::peer::cmd::Disconnect);
        }
    }
}

impl<S: 'static, M: 'static + crate::Metrics> Handler<cmd::Broadcast> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Broadcast, _: &mut Self::Context) {
        self.broadcast(&msg.0, None);
    }
}

impl<S: 'static, M: 'static + crate::Metrics> Handler<cmd::Metrics<M>> for Network<S, M>
where
    M: actix::dev::MessageResponse<Network<S, M>, cmd::Metrics<M>>,
{
    type Result = M;

    fn handle(&mut self, _: cmd::Metrics<M>, _: &mut Self::Context) -> Self::Result {
        self.metrics.clone()
    }
}
