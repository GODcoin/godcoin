use actix::prelude::*;
use godcoin::prelude::*;

mod common;

use common::TestMinter;

#[test]
fn empty_blockchain() {
    System::run(|| {
        let minter = TestMinter::new();
        assert!(minter.chain().get_block(0).is_none());
        System::current().stop();
    })
    .unwrap();
}

#[test]
fn get_block() {
    System::run(|| {
        let minter = TestMinter::new();

        let res = minter.request(MsgRequest::GetBlock(0));
        match res {
            MsgResponse::Error(kind, msg) => {
                assert_eq!(kind, net::ErrorKind::InvalidHeight);
                assert_eq!(msg, None);
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
