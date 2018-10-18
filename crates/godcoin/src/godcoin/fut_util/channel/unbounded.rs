use futures::{
    task::AtomicTask,
    Stream,
    Async,
    Poll
};
use crossbeam_channel::{
    internal::channel::{RecvNonblocking, recv_nonblocking},
    self
};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub fn unbounded<T>() -> (ChannelSender<T>, ChannelStream<T>) {
    let (tx, rx) = crossbeam_channel::unbounded::<T>();
    let tx = ChannelSender {
        tx,
        task: Arc::new(AtomicTask::new())
    };
    let rx = ChannelStream {
        rx,
        task: Arc::clone(&tx.task),
        is_done: Arc::new(AtomicBool::new(false))
    };
    (tx, rx)
}

pub struct ChannelSender<T> {
    tx: crossbeam_channel::Sender<T>,
    task: Arc<AtomicTask>
}

impl<T> ChannelSender<T> {
    pub fn send(&self, msg: T) {
        self.tx.send(msg);
        self.task.notify();
    }
}

impl<T> Clone for ChannelSender<T> {
    fn clone(&self) -> ChannelSender<T> {
        ChannelSender {
            tx: self.tx.clone(),
            task: Arc::clone(&self.task)
        }
    }
}

impl<T> Drop for ChannelSender<T> {
    fn drop(&mut self) {
        self.task.notify();
    }
}

#[must_use = "streams do nothing unless polled"]
pub struct ChannelStream<T> {
    rx: crossbeam_channel::Receiver<T>,
    task: Arc<AtomicTask>,
    is_done: Arc<AtomicBool>
}

impl<T> ChannelStream<T> {
    pub fn is_done(&self) -> bool {
        self.is_done.load(Ordering::Acquire)
    }
}

impl<T> Clone for ChannelStream<T> {
    fn clone(&self) -> ChannelStream<T> {
        ChannelStream {
            rx: self.rx.clone(),
            task: Arc::clone(&self.task),
            is_done: Arc::clone(&self.is_done)
        }
    }
}

impl<T> Stream for ChannelStream<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.task.register();
        match recv_nonblocking(&self.rx) {
            RecvNonblocking::Message(msg) => {
                Ok(Async::Ready(Some(msg)))
            },
            RecvNonblocking::Empty => {
                Ok(Async::NotReady)
            },
            RecvNonblocking::Closed => {
                self.is_done.store(true, Ordering::Release);
                Ok(Async::Ready(None))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use futures::Future;
    use std::mem;
    use super::*;

    #[test]
    fn test_channel_sending() {
        let (tx, rx) = unbounded::<usize>();
        let num = Arc::new(AtomicUsize::new(0));

        tx.send(0);
        rx.clone().and_then(|val| {
            assert_eq!(val, num.load(Ordering::Relaxed));
            Ok(())
        }).wait().next();

        num.store(1, Ordering::Relaxed);
        tx.send(1);
        rx.clone().and_then(|val| {
            assert_eq!(val, num.load(Ordering::Relaxed));
            Ok(())
        }).wait().next();

        mem::drop(tx);
        rx.clone().for_each(|_| { Ok(()) }).wait().unwrap();
        assert!(rx.is_done());
    }

    #[test]
    fn test_channel_is_done() {
        let (_, rx) = unbounded::<()>();
        rx.clone().for_each(|_| { Ok(()) }).wait().unwrap();
        assert!(rx.is_done());
    }
}
