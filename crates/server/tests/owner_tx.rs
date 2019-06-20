use actix::prelude::*;
use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn owner_tx_minter_key_change() {
    System::run(|| {
        let minter = TestMinter::new();

        let minter_key = KeyPair::gen();
        let wallet_key = KeyPair::gen();

        let tx = {
            let mut tx = OwnerTx {
                base: create_tx_header(TxType::OWNER, "0.0000 GRAEL"),
                minter: minter_key.0,
                wallet: wallet_key.0.into(),
                script: minter.genesis_info().script.clone(),
            };
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            TxVariant::OwnerTx(tx)
        };

        let fut = minter.request(MsgRequest::Broadcast(tx.clone()));
        Arbiter::spawn(
            fut.and_then(move |res| {
                assert!(!res.is_err(), format!("{:?}", res));
                assert_eq!(res, MsgResponse::Broadcast());

                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let owner = minter.chain().get_owner();
                assert_eq!(tx, TxVariant::OwnerTx(owner));

                minter.produce_block()
            })
            .and_then(|res| {
                // Minter key changed, should fail
                assert_eq!(res.unwrap_err(), verify::BlockErr::InvalidSignature);

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}

#[test]
fn owner_tx_deny_mint_tokens() {
    System::run(|| {
        let minter = TestMinter::new();

        let wallet_key = KeyPair::gen();

        let tx = {
            let mut tx = OwnerTx {
                base: create_tx_header(TxType::OWNER, "0.0000 GRAEL"),
                minter: minter.genesis_info().minter_key.0.clone(),
                wallet: (&wallet_key.0).into(),
                script: minter.genesis_info().script.clone(),
            };
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            TxVariant::OwnerTx(tx)
        };

        let fut = minter.request(MsgRequest::Broadcast(tx.clone()));
        Arbiter::spawn(
            fut.and_then(move |res| {
                assert!(!res.is_err(), format!("{:?}", res));
                assert_eq!(res, MsgResponse::Broadcast());

                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let owner = minter.chain().get_owner();
                assert_eq!(tx, TxVariant::OwnerTx(owner));

                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let mut tx = MintTx {
                    base: create_tx_header(TxType::MINT, "0.0000 GRAEL"),
                    to: (&minter.genesis_info().script).into(),
                    amount: get_asset("10.0000 GRAEL"),
                    attachment: vec![],
                    attachment_name: "".to_owned(),
                    // This is the old owner script, validation should fail
                    script: minter.genesis_info().script.clone(),
                };
                tx.append_sign(&wallet_key);

                minter.request(MsgRequest::Broadcast(TxVariant::MintTx(tx)))
            })
            .and_then(|res| {
                assert_eq!(
                    res,
                    MsgResponse::Error(net::ErrorKind::TxValidation(
                        verify::TxErr::ScriptHashMismatch
                    ))
                );

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}

#[test]
fn owner_tx_accept_mint_tokens() {
    System::run(|| {
        let minter = TestMinter::new();

        let wallet_key = KeyPair::gen();

        let tx = {
            let mut tx = OwnerTx {
                base: create_tx_header(TxType::OWNER, "0.0000 GRAEL"),
                minter: minter.genesis_info().minter_key.0.clone(),
                wallet: (&wallet_key.0).into(),
                script: minter.genesis_info().script.clone(),
            };
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            TxVariant::OwnerTx(tx)
        };

        let fut = minter.request(MsgRequest::Broadcast(tx.clone()));
        Arbiter::spawn(
            fut.and_then(move |res| {
                assert!(!res.is_err(), format!("{:?}", res));
                assert_eq!(res, MsgResponse::Broadcast());

                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let owner = minter.chain().get_owner();
                assert_eq!(tx, TxVariant::OwnerTx(owner));

                minter.produce_block().map(|_| minter)
            })
            .and_then({
                let wallet_key = wallet_key.clone();
                move |minter| {
                    let mut tx = MintTx {
                        base: create_tx_header(TxType::MINT, "0.0000 GRAEL"),
                        to: wallet_key.0.clone().into(),
                        amount: get_asset("1000.0000 GRAEL"),
                        attachment: vec![],
                        attachment_name: "".to_owned(),
                        script: wallet_key.0.clone().into(),
                    };
                    tx.append_sign(&wallet_key);
                    let tx = TxVariant::MintTx(tx);

                    minter
                        .request(MsgRequest::Broadcast(tx))
                        .map(|res| (res, minter))
                }
            })
            .and_then(|(res, minter)| {
                assert_eq!(res, MsgResponse::Broadcast());
                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let chain = minter.chain();
                let props = chain.get_properties();
                let expected_bal = get_asset("2000.0000 GRAEL");
                assert_eq!(props.token_supply, expected_bal);

                let bal = chain.get_balance(&wallet_key.0.clone().into(), &[]);
                assert_eq!(bal, Some(get_asset("1000.0000 GRAEL")));

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}
