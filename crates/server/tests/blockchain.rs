use godcoin::{
    blockchain::{error::TxErr, index::TxManager},
    constants,
    prelude::{net::ErrorKind, script::EvalErrType, *},
};

mod common;
pub use common::*;

#[test]
fn fresh_blockchain() {
    let minter = TestMinter::new();
    let chain = minter.chain();
    assert!(chain.get_block(0).is_some());
    assert!(chain.get_block(1).is_some());
    assert_eq!(chain.get_chain_height(), 1);

    let owner = match chain.get_owner() {
        TxVariant::V0(tx) => match tx {
            TxVariantV0::OwnerTx(tx) => tx,
            _ => unreachable!(),
        },
    };

    assert_eq!(owner.minter, minter.genesis_info().minter_key.0);
    assert_eq!(
        owner.script,
        script::Builder::new()
            .push(script::FnBuilder::new(0, OpFrame::OpDefine).push(OpFrame::False))
            .build()
            .unwrap()
    );
    assert_eq!(owner.wallet, (&minter.genesis_info().script).into());

    assert!(chain.get_block(2).is_none());
    assert_eq!(chain.index_status(), IndexStatus::Complete);
}

#[test]
fn reindexed_blockchain() {
    let mut minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let from_bal = minter.chain().get_balance(&from_addr, &[]).unwrap();
    let to_addr = KeyPair::gen();
    let amount = get_asset("1.00000 TEST");

    // Create a tx we expect to be reindexed
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_addr.clone(),
            to: (&to_addr.0).into(),
            amount,
            memo: vec![],
            script: minter.genesis_info().script.clone(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let tx_data = TxPrecompData::from_tx(tx.clone());

    {
        // Broadcast the tx
        let res = minter.send_req(rpc::Request::Broadcast(tx.clone()));
        assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
        minter.produce_block().unwrap();
    }

    {
        // Unindex the blockchain
        minter.unindexed();
        let chain = minter.chain();
        let manager = TxManager::new(chain.indexer());

        assert_eq!(chain.index_status(), IndexStatus::None);
        assert!(chain.get_block(0).is_none());
        assert!(!manager.has(tx_data.txid()));
    }

    // Test the reindexed status from here
    minter.reindex();
    let chain = minter.chain();
    let manager = TxManager::new(chain.indexer());

    assert_eq!(chain.index_status(), IndexStatus::Complete);
    assert!(chain.get_block(0).is_some());
    assert!(chain.get_block(1).is_some());
    assert!(chain.get_block(2).is_some());
    assert!(chain.get_block(3).is_none());
    assert_eq!(chain.get_chain_height(), 2);
    assert!(manager.has(tx_data.txid()));

    let owner = match chain.get_owner() {
        TxVariant::V0(tx) => match tx {
            TxVariantV0::OwnerTx(tx) => tx,
            _ => unreachable!(),
        },
    };
    assert_eq!(owner.minter, minter.genesis_info().minter_key.0);
    assert_eq!(
        owner.script,
        script::Builder::new()
            .push(script::FnBuilder::new(0, OpFrame::OpDefine).push(OpFrame::False))
            .build()
            .unwrap()
    );
    assert_eq!(owner.wallet, (&minter.genesis_info().script).into());

    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(amount));

    // The fee transfers back to the minter wallet in the form of a reward tx so it
    // must not be subtracted during the assertion
    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, from_bal.checked_sub(amount));

    // Test to ensure that after a reindex the tx cannot be rebroadcasted
    let res = minter.send_req(rpc::Request::Broadcast(tx.clone()));

    assert_eq!(res, Some(Err(ErrorKind::TxValidation(TxErr::TxDupe))));
}

#[test]
fn tx_dupe() {
    let minter = TestMinter::new();
    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    tx.append_sign(&minter.genesis_info().wallet_keys[1]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TxDupe)));
}

#[test]
fn tx_no_dupe_with_different_nonce() {
    let minter = TestMinter::new();
    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    tx.append_sign(&minter.genesis_info().wallet_keys[1]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));

    match &mut tx {
        TxVariant::V0(ref mut tx) => {
            tx.nonce = tx.nonce.wrapping_add(1);
            tx.signature_pairs = vec![];
        }
    }
    tx.append_sign(&minter.genesis_info().wallet_keys[1]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));
}

#[test]
fn tx_sig_validation_err_with_different_nonce() {
    let minter = TestMinter::new();
    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    tx.append_sign(&minter.genesis_info().wallet_keys[1]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));

    match &mut tx {
        TxVariant::V0(ref mut tx) => {
            tx.nonce = tx.nonce.wrapping_add(1);
        }
    }
    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    match res {
        Err(ErrorKind::TxValidation(TxErr::ScriptEval(e))) => {
            assert_eq!(e.err, EvalErrType::ScriptRetFalse)
        }
        _ => panic!("Assertion failed, got {:?}", res),
    }
}

#[test]
fn tx_expired() {
    let minter = TestMinter::new();
    let expiry = godcoin::get_epoch_time();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header_with_expiry("0.00000 TEST", expiry),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TxExpired)));

    match &mut tx {
        TxVariant::V0(ref mut tx) => {
            tx.expiry = expiry - 1;
        }
    }
    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TxExpired)));
}

#[test]
fn tx_expiry_far_in_the_future() {
    let minter = TestMinter::new();
    let expiry = godcoin::get_epoch_time();

    let tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header_with_expiry(
            "0.00000 TEST",
            expiry + constants::TX_MAX_EXPIRY_TIME + 1,
        ),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TxExpired)));
}

#[test]
fn tx_script_too_large_err() {
    let minter = TestMinter::new();

    let tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: Script::new((0..=constants::MAX_SCRIPT_BYTE_SIZE).map(|_| 0).collect()),
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TxTooLarge)));
}

#[test]
fn tx_too_many_signatures_err() {
    let minter = TestMinter::new();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: Script::new(vec![]),
    }));
    (0..=constants::MAX_TX_SIGNATURES).for_each(|_| tx.append_sign(&KeyPair::gen()));

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TooManySignatures)));
}

#[test]
fn tx_with_bad_chain_id() {
    fn manual_sign(key_pair: &KeyPair, tx: &mut TxVariant, chain_id: [u8; 2]) {
        let mut buf = Vec::with_capacity(4096);
        tx.serialize_without_sigs(&mut buf);

        let digest = {
            let mut hasher = DoubleSha256::new();
            hasher.update(&chain_id);
            hasher.update(&buf);
            hasher.finalize()
        };
        let sig = key_pair.sign(digest.as_ref());

        match tx {
            TxVariant::V0(ref mut tx) => {
                tx.signature_pairs.push(sig);
            }
        }
    };

    let minter = TestMinter::new();
    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    {
        // Test manual signing with the default chain ID to ensure manual_sign is correct
        manual_sign(
            &minter.genesis_info().wallet_keys[1],
            &mut tx,
            constants::CHAIN_ID,
        );
        manual_sign(
            &minter.genesis_info().wallet_keys[0],
            &mut tx,
            constants::CHAIN_ID,
        );

        let res = minter
            .send_req(rpc::Request::Broadcast(tx.clone()))
            .unwrap();
        assert_eq!(res, Ok(rpc::Response::Broadcast));
    }
    {
        // Test manual signing with incorrect chain ID to ensure these transactions cannot be broadcasted
        match &mut tx {
            TxVariant::V0(ref mut tx) => {
                tx.nonce = tx.nonce.wrapping_add(1);
                tx.signature_pairs = vec![];
            }
        }
        let mut new_id = [0x00, 0x00];
        new_id[0] = constants::CHAIN_ID[0].wrapping_add(1);
        new_id[1] = constants::CHAIN_ID[0].wrapping_add(1);
        manual_sign(&minter.genesis_info().wallet_keys[1], &mut tx, new_id);
        manual_sign(&minter.genesis_info().wallet_keys[0], &mut tx, new_id);

        let res = minter
            .send_req(rpc::Request::Broadcast(tx.clone()))
            .unwrap();
        match res {
            Err(ErrorKind::TxValidation(TxErr::ScriptEval(e))) => {
                assert_eq!(e.err, EvalErrType::ScriptRetFalse)
            }
            _ => panic!("Assertion failed, got {:?}", res),
        }
    }
}
