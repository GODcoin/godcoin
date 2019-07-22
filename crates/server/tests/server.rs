use godcoin::{
    constants,
    prelude::{net::ErrorKind, *},
};
use std::io::Cursor;

mod common;
pub use common::*;

#[test]
fn successful_broadcast() {
    let minter = TestMinter::new();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 GRAEL"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 GRAEL"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    tx.append_sign(&minter.genesis_info().wallet_keys[1]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter.request(MsgRequest::Broadcast(tx));
    assert_eq!(res, MsgResponse::Broadcast);
}

#[test]
fn get_properties() {
    let minter = TestMinter::new();
    let res = minter.request(MsgRequest::GetProperties);
    let chain_props = minter.chain().get_properties();
    assert!(!res.is_err());
    assert_eq!(res, MsgResponse::GetProperties(chain_props));
}

#[test]
fn get_block() {
    let minter = TestMinter::new();
    let res = minter.request(MsgRequest::GetBlock(0));

    assert!(!res.is_err());

    let other = minter.chain().get_block(0).unwrap();
    assert_eq!(res, MsgResponse::GetBlock((*other).clone()));

    let res = minter.request(MsgRequest::GetBlock(2));
    assert!(res.is_err());
    assert_eq!(res, MsgResponse::Error(ErrorKind::InvalidHeight));
}

#[test]
fn get_address_info() {
    let minter = TestMinter::new();
    let addr = (&minter.genesis_info().script).into();
    let res = minter.request(MsgRequest::GetAddressInfo(addr));
    assert!(!res.is_err());

    let expected = MsgResponse::GetAddressInfo(AddressInfo {
        net_fee: constants::GRAEL_FEE_MIN,
        addr_fee: constants::GRAEL_FEE_MIN
            .mul(constants::GRAEL_FEE_MULT)
            .unwrap(),
        balance: get_asset("1000.00000 GRAEL"),
    });
    assert_eq!(res, expected);
}

#[test]
fn error_with_bytes_remaining() {
    let minter = TestMinter::new();

    let body = {
        let req = net::RequestType::Batch(vec![MsgRequest::GetBlock(0)]);
        let mut buf = Vec::with_capacity(4096);
        req.serialize(&mut buf);

        // Set the batch len to 0
        buf[1..=4].iter_mut().for_each(|x| *x = 0);

        buf
    };

    // Confirm the length is actually 0 in case the binary format changes
    match net::RequestType::deserialize(&mut Cursor::new(&body)).unwrap() {
        net::RequestType::Batch(reqs) => assert_eq!(reqs.len(), 0),
        _ => panic!("Expected batch request type"),
    }

    let res = minter.raw_request(body);
    let res = res.unwrap_single();
    assert!(res.is_err());
    assert_eq!(res, MsgResponse::Error(ErrorKind::BytesRemaining));
}

#[test]
fn batch_preserves_order() {
    let minter = TestMinter::new();
    let responses = minter.batch_request(vec![
        MsgRequest::GetBlock(0),
        MsgRequest::GetBlock(2),
        MsgRequest::GetBlock(1),
    ]);
    assert_eq!(responses.len(), 3);

    let block_0 = minter.chain().get_block(0).unwrap();
    let block_1 = minter.chain().get_block(1).unwrap();

    assert_eq!(responses[0], MsgResponse::GetBlock((*block_0).clone()));
    assert_eq!(
        responses[1],
        MsgResponse::Error(net::ErrorKind::InvalidHeight)
    );
    assert_eq!(responses[2], MsgResponse::GetBlock((*block_1).clone()));
}
