use actix::prelude::*;
use godcoin::prelude::{net::ErrorKind, *};
use std::io::Cursor;

mod common;
pub use common::*;

#[test]
fn error_with_bytes_remaining() {
    System::run(|| {
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

        let fut = minter.raw_request(body);
        Arbiter::spawn(fut.and_then(move |res| {
            let res = res.unwrap_single();
            assert!(res.is_err());
            assert_eq!(res, MsgResponse::Error(ErrorKind::BytesRemaining));

            System::current().stop();
            Ok(())
        }));
    })
    .unwrap();
}

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
fn successful_broadcast() {
    System::run(|| {
        let minter = TestMinter::new();

        let mut tx = MintTx {
            base: create_tx_header(TxType::MINT, "0.0000 GRAEL"),
            to: (&minter.genesis_info().script).into(),
            amount: get_asset("10.0000 GRAEL"),
            attachment: vec![],
            attachment_name: "".to_owned(),
            script: minter.genesis_info().script.clone(),
        };

        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);

        let tx = TxVariant::MintTx(tx);
        let fut = minter.request(MsgRequest::Broadcast(tx));
        Arbiter::spawn(fut.and_then(move |res| {
            assert_eq!(res, MsgResponse::Broadcast());

            System::current().stop();
            Ok(())
        }));
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
