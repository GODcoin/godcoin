use godcoin::{
    constants,
    prelude::{net::ErrorKind, *},
};
use godcoin_server::WsState;
use std::{collections::BTreeSet, net::SocketAddr};

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

    let res = minter.request(RequestBody::Broadcast(tx));
    assert_eq!(res, ResponseBody::Broadcast);
}

#[test]
fn get_properties() {
    let minter = TestMinter::new();
    let res = minter.request(RequestBody::GetProperties);
    let chain_props = minter.chain().get_properties();
    assert!(!res.is_err());
    assert_eq!(res, ResponseBody::GetProperties(chain_props));
}

#[test]
fn get_block_unfiltered() {
    let minter = TestMinter::new();
    let res = minter.request(RequestBody::GetBlock(0));

    assert!(!res.is_err());

    let other = minter.chain().get_block(0).unwrap();
    assert_eq!(res, ResponseBody::GetBlock(FilteredBlock::Block(other)));

    let res = minter.request(RequestBody::GetBlock(2));
    assert!(res.is_err());
    assert_eq!(res, ResponseBody::Error(ErrorKind::InvalidHeight));
}

#[test]
fn get_block_filtered() {
    let mut state = create_uninit_state();
    let minter = TestMinter::new();

    let mut filter = BTreeSet::<ScriptHash>::new();
    filter.insert((&minter.genesis_info().script).into());
    let res = minter
        .send_request(
            &mut state,
            net::Request {
                id: 0,
                body: RequestBody::SetBlockFilter(BlockFilter::Addr(filter.clone())),
            },
        )
        .body;
    assert_eq!(res, ResponseBody::SetBlockFilter);
    assert!(!res.is_err());

    assert_eq!(state.filter(), Some(&filter));

    {
        let block = minter.chain().get_block(1).unwrap();
        let res = minter
            .send_request(
                &mut state,
                net::Request {
                    id: 0,
                    body: RequestBody::GetBlock(1),
                },
            )
            .body;
        assert_eq!(res, ResponseBody::GetBlock(FilteredBlock::Block(block)));
    }

    {
        // Produce block 2, should be filtered
        minter.produce_block().unwrap();

        let tx = {
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: create_tx_header("1.00000 GRAEL"),
                from: (&minter.genesis_info().script).into(),
                to: (&KeyPair::gen().0).into(),
                amount: get_asset("1.00000 GRAEL"),
                memo: vec![],
                script: minter.genesis_info().script.clone(),
            }));
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            tx
        };
        let res = minter.request(RequestBody::Broadcast(tx));
        assert_eq!(res, ResponseBody::Broadcast);
        // Produce block 3, should not be filtered
        minter.produce_block().unwrap();
    }

    {
        let block = minter.chain().get_block(2).unwrap();
        let res = minter
            .send_request(
                &mut state,
                net::Request {
                    id: 0,
                    body: RequestBody::GetBlock(2),
                },
            )
            .body;

        let signer = block.signer().unwrap().clone();
        assert_eq!(
            res,
            ResponseBody::GetBlock(FilteredBlock::Header((block.header(), signer)))
        );
    }

    {
        let block = minter.chain().get_block(3).unwrap();
        let res = minter
            .send_request(
                &mut state,
                net::Request {
                    id: 0,
                    body: RequestBody::GetBlock(3),
                },
            )
            .body;
        assert_eq!(res, ResponseBody::GetBlock(FilteredBlock::Block(block)));
    }
}

#[test]
fn get_block_header() {
    let minter = TestMinter::new();
    let res = minter.request(RequestBody::GetBlockHeader(0));
    assert!(!res.is_err());

    let other = minter.chain().get_block(0).unwrap();
    let header = other.header();
    let signer = other.signer().unwrap().clone();
    assert_eq!(res, ResponseBody::GetBlockHeader { header, signer });

    let res = minter.request(RequestBody::GetBlockHeader(2));
    assert!(res.is_err());
    assert_eq!(res, ResponseBody::Error(ErrorKind::InvalidHeight));
}

#[test]
fn get_address_info() {
    let minter = TestMinter::new();
    let addr = (&minter.genesis_info().script).into();
    let res = minter.request(RequestBody::GetAddressInfo(addr));
    assert!(!res.is_err());

    let expected = ResponseBody::GetAddressInfo(AddressInfo {
        net_fee: constants::GRAEL_FEE_MIN,
        addr_fee: constants::GRAEL_FEE_MIN
            .checked_mul(constants::GRAEL_FEE_MULT)
            .unwrap(),
        balance: get_asset("1000.00000 GRAEL"),
    });
    assert_eq!(res, expected);
}

#[test]
fn error_with_bytes_remaining() {
    let minter = TestMinter::new();

    let buf = {
        let req = net::Request {
            id: 123456789,
            body: RequestBody::GetBlock(0),
        };
        let mut buf = Vec::with_capacity(4096);
        req.serialize(&mut buf);

        // Push an extra byte that should trigger the error
        buf.push(0);

        buf
    };

    let res = minter.raw_request(&mut create_uninit_state(), buf);
    assert!(res.body.is_err());
    assert_eq!(res.id, 123456789);
    assert_eq!(res.body, ResponseBody::Error(ErrorKind::BytesRemaining));
}

#[test]
fn eof_returns_max_u32_id() {
    let minter = TestMinter::new();

    let buf = {
        let req = net::Request {
            id: 123456789,
            body: RequestBody::GetBlock(0),
        };
        let mut buf = Vec::with_capacity(4096);
        req.serialize(&mut buf);

        // Delete an extra byte causing an EOF error triggering a failure to deserialize the message
        buf.truncate(buf.len() - 1);

        buf
    };

    let res = minter.raw_request(&mut create_uninit_state(), buf);
    assert!(res.body.is_err());
    assert_eq!(res.id, u32::max_value());
    assert_eq!(res.body, ResponseBody::Error(ErrorKind::Io));
}

#[test]
fn u32_max_val_with_valid_request_fails() {
    let minter = TestMinter::new();
    let addr = (&minter.genesis_info().script).into();

    let buf = {
        let req = net::Request {
            id: u32::max_value(),
            body: RequestBody::GetAddressInfo(addr),
        };
        let mut buf = Vec::with_capacity(4096);
        req.serialize(&mut buf);

        buf
    };
    let res = minter.raw_request(&mut create_uninit_state(), buf);

    let expected = net::Response {
        id: u32::max_value(),
        body: ResponseBody::Error(ErrorKind::Io),
    };
    assert_eq!(res, expected);
    assert!(res.body.is_err());
}

#[test]
fn response_id_matches_request() {
    let minter = TestMinter::new();
    let addr = (&minter.genesis_info().script).into();

    let buf = {
        let req = net::Request {
            id: 123456789,
            body: RequestBody::GetAddressInfo(addr),
        };
        let mut buf = Vec::with_capacity(4096);
        req.serialize(&mut buf);

        buf
    };
    let res = minter.raw_request(&mut create_uninit_state(), buf);

    assert!(!res.body.is_err());

    let expected = net::Response {
        id: 123456789,
        body: ResponseBody::GetAddressInfo(AddressInfo {
            net_fee: constants::GRAEL_FEE_MIN,
            addr_fee: constants::GRAEL_FEE_MIN
                .checked_mul(constants::GRAEL_FEE_MULT)
                .unwrap(),
            balance: get_asset("1000.00000 GRAEL"),
        }),
    };
    assert_eq!(res, expected);
}

fn create_uninit_state() -> WsState {
    let (tx, _) = futures::sync::mpsc::unbounded();
    WsState::new(SocketAddr::from(([127, 0, 0, 1], 7777)), tx)
}
