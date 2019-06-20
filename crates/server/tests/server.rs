use actix::prelude::*;
use godcoin::prelude::{net::ErrorKind, *};

mod common;

pub use common::*;

#[test]
fn get_block() {
    System::run(|| {
        let minter = TestMinter::new();
        let fut = minter.request(MsgRequest::GetBlock(0));
        Arbiter::spawn(
            fut.and_then(move |res| {
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

#[test]
fn batch_preserves_order() {
    System::run(|| {
        let minter = TestMinter::new();
        let fut = minter.batch_request(vec![
            MsgRequest::GetBlock(0),
            MsgRequest::GetBlock(2),
            MsgRequest::GetBlock(1),
        ]);
        Arbiter::spawn(fut.and_then(move |responses| {
            assert_eq!(responses.len(), 3);

            let block_0 = minter.chain().get_block(0).unwrap();
            let block_1 = minter.chain().get_block(1).unwrap();

            assert_eq!(responses[0], MsgResponse::GetBlock((*block_0).clone()));
            assert_eq!(
                responses[1],
                MsgResponse::Error(net::ErrorKind::InvalidHeight)
            );
            assert_eq!(responses[2], MsgResponse::GetBlock((*block_1).clone()));

            System::current().stop();
            Ok(())
        }));
    })
    .unwrap();
}
