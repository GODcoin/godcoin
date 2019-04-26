use godcoin::{prelude::*, test::*};
use godcoin_server::handle_request;

#[test]
fn empty_blockchain() {
    let chain = TestBlockchain::new();
    assert!(chain.get_block(0).is_none());
}

#[test]
fn get_properties() {
    let chain = TestBlockchain::new();

    let res = handle_request(&chain, MsgRequest::GetBlock(0));
    match res {
        MsgResponse::Error(kind, msg) => {
            assert_eq!(kind, net::ErrorKind::InvalidHeight);
            assert_eq!(msg, None);
        }
        _ => panic!("Unexpected response: {:?}", res),
    }

    let res = handle_request(&chain, MsgRequest::GetBlock(0));
    match res {
        MsgResponse::Error(kind, msg) => {
            assert_eq!(kind, net::ErrorKind::InvalidHeight);
            assert_eq!(msg, None);
        }
        _ => panic!("Unexpected response: {:?}", res),
    }
}
