use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::collections::HashMap;
use futures::sync::oneshot;
use parking_lot::Mutex;
use tokio::prelude::*;

use super::unbounded;

pub fn tracked<In, Out, F>(req_handler: F) -> ChannelTracker<In, Out>
        where In: Send + 'static,
                Out: Send + 'static,
                F: Fn(In) -> Out + Send + 'static {
    let (tx, rx) = unbounded::unbounded::<ChannelMessage<In>>();
    let tracker = ChannelTracker {
        id: Arc::new(AtomicUsize::new(0)),
        tx,
        messages: Arc::new(Mutex::new(HashMap::with_capacity(16)))
    };

    ::tokio::spawn(rx.for_each({
        let msgs = Arc::clone(&tracker.messages);
        move |msg| {
            let res = req_handler(msg.msg);
            let handler = msgs.lock().remove(&msg.id).unwrap();
            handler.send(res).map_err(|_| {
                "failed to send msg to handler"
            }).unwrap();
            Ok(())
        }
    }));

    tracker
}

pub struct ChannelTracker<In, Out> {
    id: Arc<AtomicUsize>,
    tx: unbounded::ChannelSender<ChannelMessage<In>>,
    messages: Arc<Mutex<HashMap<usize, oneshot::Sender<Out>>>>
}

impl<In, Out> Clone for ChannelTracker<In, Out> {
    fn clone(&self) -> Self {
        ChannelTracker {
            id: Arc::clone(&self.id),
            tx: self.tx.clone(),
            messages: Arc::clone(&self.messages)
        }
    }
}

impl<In, Out> ChannelTracker<In, Out> {
    pub fn send(&self, msg: In) -> oneshot::Receiver<Out> {
        let (tx, rx) = oneshot::channel::<Out>();
        let id = self.id.fetch_add(1, Ordering::AcqRel);
        self.messages.lock().insert(id, tx);
        self.tx.send(ChannelMessage {
            id,
            msg
        });
        rx
    }
}

struct ChannelMessage<T> {
    id: usize,
    msg: T
}
