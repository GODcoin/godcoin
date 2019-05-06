use actix::prelude::*;
use godcoin::prelude::*;

mod common;

use common::TestMinter;

#[test]
fn fresh_blockchain() {
    System::run(|| {
        let minter = TestMinter::new();
        let chain = minter.chain();
        assert!(chain.get_block(0).is_some());
        assert_eq!(chain.get_chain_height(), 0);

        let owner = chain.get_owner();
        assert_eq!(owner.minter, minter.genesis_info().minter_key.0);
        assert_eq!(
            owner.script,
            script::Builder::new().push(OpFrame::False).build()
        );
        assert_eq!(owner.wallet, (&minter.genesis_info().script).into());

        assert!(chain.get_block(1).is_none());
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
            MsgResponse::GetBlock(block) => {
                assert_eq!(block.height, 0);
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
