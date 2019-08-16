use log::error;
use std::{io, time::Duration};
use tokio::{
    clock,
    net::{tcp::Incoming, TcpStream},
    prelude::*,
    timer::Delay,
};

fn is_connection_error(e: &io::Error) -> bool {
    match e.kind() {
        io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionReset => true,
        _ => false,
    }
}

pub struct ListenForever {
    stream: Incoming,
    timeout: Option<Delay>,
}

impl ListenForever {
    pub fn new(stream: Incoming) -> Self {
        Self {
            stream,
            timeout: None,
        }
    }
}

impl Stream for ListenForever {
    type Item = TcpStream;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<TcpStream>, ()> {
        if let Some(timeout) = self.timeout.as_mut() {
            match timeout.poll().unwrap() {
                Async::Ready(_) => self.timeout = None,
                Async::NotReady => return Ok(Async::NotReady),
            }
        }

        loop {
            match self.stream.poll() {
                Ok(res) => match res {
                    Async::Ready(stream) => return Ok(Async::Ready(stream)),
                    Async::NotReady => return Ok(Async::NotReady),
                },
                Err(e) => {
                    error!("Accept error = {:?}", e);
                    match e {
                        ref e if is_connection_error(e) => continue,
                        _ => {
                            let mut timeout = Delay::new(clock::now() + Duration::from_millis(500));
                            // Ensure the timeout gets registered
                            match timeout.poll().unwrap() {
                                Async::Ready(_) => continue,
                                Async::NotReady => {
                                    self.timeout = Some(timeout);
                                    return Ok(Async::NotReady);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
