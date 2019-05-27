use actix::prelude::*;
use godcoin::prelude::{net::ErrorKind, *};

mod common;

pub use common::*;

#[test]
fn get_block() {
    System::run(|| {
        let minter = TestMinter::new();
        let fut = minter.request(MsgRequest::GetBlock(0));
        Arbiter::current().send(
            fut.then(move |res| {
                let res = res.unwrap();
                assert!(!res.is_err());

                let other = minter.chain().get_block(0).unwrap();
                assert_eq!(res, MsgResponse::GetBlock((*other).clone()));

                minter.request(MsgRequest::GetBlock(2))
            })
            .then(|res| {
                let res = res.unwrap();
                assert!(res.is_err());
                assert_eq!(res, MsgResponse::Error(ErrorKind::InvalidHeight));

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}
