use net::rpc::RpcPayload;
use fut_util::channel::*;

pub type ClientSender = ChannelSender<ClientEvent>;
pub type ClientReceiver = ChannelStream<ClientEvent>;

#[derive(Debug)]
pub enum ClientEvent {
    Message(RpcPayload),
    Connect,
    Disconnect
}
