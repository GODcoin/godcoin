use actix::prelude::*;
use godcoin::prelude::*;

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
                match res {
                    MsgResponse::GetBlock(block) => {
                        assert_eq!(block.height, 0);
                        let other = minter.chain().get_block(0).unwrap();
                        assert_eq!(&block, other.as_ref());
                    }
                    _ => panic!("Unexpected response: {:?}", res),
                }

                minter.request(MsgRequest::GetBlock(1))
            })
            .then(|res| {
                let res = res.unwrap();
                assert!(res.is_err());
                match res {
                    MsgResponse::Error(kind) => {
                        assert_eq!(kind, net::ErrorKind::InvalidHeight);
                    }
                    _ => panic!("Unexpected response: {:?}", res),
                }
                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}
