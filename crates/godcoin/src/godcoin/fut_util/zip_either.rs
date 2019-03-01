use futures::{stream::Fuse, Async, Poll, Stream};

#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct ZipEither<S1: Stream, S2: Stream> {
    stream1: Fuse<S1>,
    stream2: Fuse<S2>,
    queued1: Option<S1::Item>,
    queued2: Option<S2::Item>,
}

impl<S1: Stream, S2: Stream<Error = S1::Error>> ZipEither<S1, S2> {
    pub fn new(stream1: S1, stream2: S2) -> ZipEither<S1, S2> {
        ZipEither {
            stream1: stream1.fuse(),
            stream2: stream2.fuse(),
            queued1: None,
            queued2: None,
        }
    }
}

impl<S1, S2> Stream for ZipEither<S1, S2>
where
    S1: Stream,
    S2: Stream<Error = S1::Error>,
{
    type Item = (Option<S1::Item>, Option<S2::Item>);
    type Error = S1::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.queued1.is_none() {
            match self.stream1.poll()? {
                Async::Ready(Some(item1)) => self.queued1 = Some(item1),
                Async::Ready(None) | Async::NotReady => {}
            }
        }
        if self.queued2.is_none() {
            match self.stream2.poll()? {
                Async::Ready(Some(item2)) => self.queued2 = Some(item2),
                Async::Ready(None) | Async::NotReady => {}
            }
        }

        if self.queued1.is_some() || self.queued2.is_some() {
            let pair = (self.queued1.take(), self.queued2.take());
            Ok(Async::Ready(Some(pair)))
        } else if self.stream1.is_done() || self.stream2.is_done() {
            Ok(Async::Ready(None))
        } else {
            Ok(Async::NotReady)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::fut_util::*;

    #[test]
    pub fn test_either_left() {
        let (tx1, rx1) = channel::unbounded::<()>();
        let (tx2, rx2) = channel::unbounded::<()>();
        let ran = AtomicBool::new(false);

        let zip = ZipEither::new(rx1, rx2);
        tx1.send(());
        zip.and_then(|(left, right)| {
            ran.store(true, Ordering::Relaxed);
            assert!(left.is_some());
            assert!(right.is_none());
            Ok(())
        })
        .wait()
        .next();

        mem::drop(tx2);
        assert!(ran.load(Ordering::Relaxed));
    }

    #[test]
    pub fn test_either_right() {
        let (tx1, rx1) = channel::unbounded::<()>();
        let (tx2, rx2) = channel::unbounded::<()>();
        let ran = AtomicBool::new(false);

        let zip = ZipEither::new(rx1, rx2);
        tx2.send(());
        zip.and_then(|(left, right)| {
            ran.store(true, Ordering::Relaxed);
            assert!(left.is_none());
            assert!(right.is_some());
            Ok(())
        })
        .wait()
        .next();

        mem::drop(tx1);
        assert!(ran.load(Ordering::Relaxed));
    }

    #[test]
    pub fn test_both() {
        let (tx1, rx1) = channel::unbounded::<()>();
        let (tx2, rx2) = channel::unbounded::<()>();
        let ran = AtomicBool::new(false);

        let zip = ZipEither::new(rx1, rx2);
        tx1.send(());
        tx2.send(());
        zip.and_then(|(left, right)| {
            ran.store(true, Ordering::Relaxed);
            assert!(left.is_some());
            assert!(right.is_some());
            Ok(())
        })
        .wait()
        .next();
        assert!(ran.load(Ordering::Relaxed));
    }
}
