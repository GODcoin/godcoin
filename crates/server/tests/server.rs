use actix::prelude::*;
use godcoin::prelude::*;

mod common;

pub use common::*;

#[test]
fn get_block() {
    System::run(|| {
        let minter = TestMinter::new();

        let res = minter.request(MsgRequest::GetBlock(0));
        match res {
            MsgResponse::GetBlock(block) => {
                assert_eq!(block.height, 0);
                let other = minter.chain().get_block(0).unwrap();
                assert_eq!(&block, other.as_ref());
            }
            _ => panic!("Unexpected response: {:?}", res),
        }

        let res = minter.request(MsgRequest::GetBlock(1));
        match res {
            MsgResponse::Error(kind, msg) => {
                assert_eq!(kind, net::ErrorKind::InvalidHeight);
                assert_eq!(msg, None);
            }
            _ => panic!("Unexpected response: {:?}", res),
        }

        System::current().stop();
    })
    .unwrap();
}
