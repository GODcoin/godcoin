use futures::sync::oneshot::Receiver;

use crate::net::rpc::RpcPayload;
use crate::fut_util::channel::*;

pub type ClientSender = ChannelTracker<ClientEvent, Option<Receiver<RpcPayload>>>;
pub type ClientReceiver = ChannelStream<ClientEvent>;

#[derive(Debug)]
pub enum ClientEvent {
    Message(Box<RpcPayload>),
    Connect,
    Disconnect
}
