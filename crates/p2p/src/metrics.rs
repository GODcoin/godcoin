use crate::Payload;
use actix::dev::*;
use std::fmt::Debug;

pub trait Metrics: Clone + Debug {
    fn on_inbound_message(&mut self, payload: &Payload);
    fn on_outbound_message(&mut self, payload: &Payload);
}

#[derive(Clone, Default, Debug)]
pub struct DummyMetrics;

impl Metrics for DummyMetrics {
    fn on_inbound_message(&mut self, _: &Payload) {}
    fn on_outbound_message(&mut self, _: &Payload) {}
}

#[derive(Clone, Default, Debug)]
pub struct BasicMetrics {
    total_inbound_msgs: usize,
    total_outbound_msgs: usize,
}

impl Metrics for BasicMetrics {
    fn on_inbound_message(&mut self, _: &Payload) {
        self.total_inbound_msgs = self.total_inbound_msgs.overflowing_add(1).0;
    }

    fn on_outbound_message(&mut self, _: &Payload) {
        self.total_outbound_msgs = self.total_outbound_msgs.overflowing_add(1).0;
    }
}

// Macro is from actix src/handler.rs
// TODO: switch to #[derive(MessageResponse)] when the derive PR is merged.
// https://github.com/actix/actix-derive/pull/14
macro_rules! SIMPLE_RESULT {
    ($type:ty) => {
        impl<A, M> MessageResponse<A, M> for $type
        where
            A: Actor,
            M: Message<Result = $type>,
        {
            fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
                if let Some(tx) = tx {
                    tx.send(self);
                }
            }
        }
    };
}

SIMPLE_RESULT!(DummyMetrics);
SIMPLE_RESULT!(BasicMetrics);
