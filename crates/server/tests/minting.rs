use actix::prelude::*;
use godcoin::prelude::*;
use godcoin_server::handle_request;

mod common;

use common::TestMinter;

#[test]
fn empty_blockchain() {
    let minter = TestMinter::new();
    assert!(minter.chain().get_block(0).is_none());
}

#[test]
fn get_block() {
    System::run(|| {
        let minter = TestMinter::new();

        let res = handle_request(minter.data(), MsgRequest::GetBlock(0));
        match res {
            MsgResponse::Error(kind, msg) => {
                assert_eq!(kind, net::ErrorKind::InvalidHeight);
                assert_eq!(msg, None);
            }
            _ => panic!("Unexpected response: {:?}", res),
        }

        let res = handle_request(minter.data(), MsgRequest::GetBlock(0));
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
