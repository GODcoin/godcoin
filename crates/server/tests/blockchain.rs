use actix::prelude::*;
use godcoin::prelude::{net::ErrorKind, verify::TxErr, *};

mod common;
pub use common::*;

#[test]
fn fresh_blockchain() {
    System::run(|| {
        let minter = TestMinter::new();
        let chain = minter.chain();
        assert!(chain.get_block(0).is_some());
        assert!(chain.get_block(1).is_some());
        assert_eq!(chain.get_chain_height(), 1);

        let owner = chain.get_owner();
        assert_eq!(owner.minter, minter.genesis_info().minter_key.0);
        assert_eq!(
            owner.script,
            script::Builder::new().push(OpFrame::False).build()
        );
        assert_eq!(owner.wallet, (&minter.genesis_info().script).into());

        assert!(chain.get_block(2).is_none());
        System::current().stop();
    })
    .unwrap();
}

#[test]
fn tx_dupe() {
    System::run(|| {
        let minter = TestMinter::new();

        let mut tx = MintTx {
            base: create_tx_header(TxType::MINT, "0.0000 GRAEL"),
            to: (&minter.genesis_info().script).into(),
            amount: get_asset("10.0000 GRAEL"),
            script: minter.genesis_info().script.clone(),
        };

        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);

        let tx = TxVariant::MintTx(tx);
        let fut = minter.request(MsgRequest::Broadcast(tx.clone()));
        System::current().arbiter().send(
            fut.and_then(move |res| {
                assert!(!res.is_err(), format!("{:?}", res));

                minter.request(MsgRequest::Broadcast(tx))
            })
            .and_then(|res| {
                assert!(res.is_err());
                assert_eq!(
                    res,
                    MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxDupe))
                );

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}

#[test]
fn tx_expired() {
    use godcoin::constants::TX_EXPIRY_TIME;

    System::run(|| {
        let minter = TestMinter::new();
        let time = util::get_epoch_ms();

        let tx = MintTx {
            base: create_tx_header_with_ts(TxType::MINT, "0.0000 GRAEL", time + TX_EXPIRY_TIME),
            to: (&minter.genesis_info().script).into(),
            amount: get_asset("10.0000 GRAEL"),
            script: minter.genesis_info().script.clone(),
        };

        let tx = TxVariant::MintTx(tx);
        let fut = minter.request(MsgRequest::Broadcast(tx));
        System::current().arbiter().send(fut.then(move |res| {
            let res = res.unwrap();
            assert!(res.is_err());
            assert_eq!(
                res,
                MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxExpired))
            );

            System::current().stop();
            Ok(())
        }));
    })
    .unwrap();
}

#[test]
fn tx_far_in_the_future() {
    System::run(|| {
        let minter = TestMinter::new();
        let time = util::get_epoch_ms();

        let tx = MintTx {
            base: create_tx_header_with_ts(TxType::MINT, "0.0000 GRAEL", time + 4000),
            to: (&minter.genesis_info().script).into(),
            amount: get_asset("10.0000 GRAEL"),
            script: minter.genesis_info().script.clone(),
        };

        let tx = TxVariant::MintTx(tx);
        let fut = minter.request(MsgRequest::Broadcast(tx));
        System::current().arbiter().send(fut.then(move |res| {
            let res = res.unwrap();
            assert!(res.is_err());
            assert_eq!(
                res,
                MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxExpired))
            );

            System::current().stop();
            Ok(())
        }));
    })
    .unwrap();
}
