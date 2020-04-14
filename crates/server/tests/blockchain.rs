use godcoin::{
    blockchain::error::TxErr,
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
    assert_eq!(owner.wallet, 0);

    assert!(chain.get_block(2).is_none());
    assert_eq!(chain.index_status(), IndexStatus::Complete);
}

#[test]
fn reindexed_blockchain() {
    let mut minter = TestMinter::new();

    let from_acc = minter.genesis_info().owner_id;
    let from_bal = minter.chain().get_account(from_acc, &[]).unwrap().balance;
    let to_acc = {
        let mut acc = Account::create_default(
            1,
            Permissions {
                threshold: 1,
                keys: vec![KeyPair::gen().0],
            },
        );
        acc.balance = get_asset("4.00000 TEST");
        minter.create_account(acc, "2.00000 TEST", true)
    };
    let amount = get_asset("1.00000 TEST");

    // Create a tx we expect to be reindexed
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_acc,
            call_fn: 1,
            args: {
                let mut args = vec![];
                args.push_u64(to_acc.id);
                args.push_asset(amount);
                args
            },
            amount,
            memo: vec![],
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

        assert_eq!(chain.index_status(), IndexStatus::None);
        assert!(chain.get_block(0).is_none());
        assert!(chain.get_account_info(0, &[]).is_none());
        assert!(!chain.indexer().has_txid(tx_data.txid()));
    }

    // Test the reindexed status from here
    minter.reindex();
    let chain = minter.chain();

    assert_eq!(chain.index_status(), IndexStatus::Complete);
    assert!(chain.get_block(0).is_some());
    assert!(chain.get_block(1).is_some());
    assert!(chain.get_block(2).is_some());
    assert!(chain.get_block(3).is_some());
    assert!(chain.get_block(4).is_none());
    assert_eq!(chain.get_chain_height(), 3);
    assert!(chain.indexer().has_txid(tx_data.txid()));

    let owner = match chain.get_owner() {
        TxVariant::V0(tx) => match tx {
            TxVariantV0::OwnerTx(tx) => tx,
            _ => unreachable!(),
        },
    };
    assert_eq!(owner.minter, minter.genesis_info().minter_key.0);
    assert_eq!(owner.wallet, 0);
    assert!(chain.get_account_info(0, &[]).is_some());

    let cur_bal = chain.get_account(to_acc.id, &[]).unwrap().balance;
    // We add the balance that the account started with
    assert_eq!(cur_bal, to_acc.balance.checked_add(amount).unwrap());

    // The fee transfers back to the minter wallet in the form of a reward tx so it must not be
    // subtracted during the assertion. We also subtract the balance that the account was created
    // with.
    let cur_bal = chain.get_account(from_acc, &[]).unwrap().balance;
    assert_eq!(
        cur_bal,
        from_bal
            .checked_sub(amount)
            .unwrap()
            .checked_sub(to_acc.balance)
            .unwrap()
    );

    // Test to ensure that after a reindex the tx cannot be rebroadcasted
    let res = minter.send_req(rpc::Request::Broadcast(tx.clone()));

    assert_eq!(res, Some(Err(ErrorKind::TxValidation(TxErr::TxDupe))));
}

#[test]
fn tx_dupe() {
    let minter = TestMinter::new();
    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
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
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
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
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
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
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
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
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Err(ErrorKind::TxValidation(TxErr::TxExpired)));
}

#[test]
fn tx_too_many_signatures_err() {
    let minter = TestMinter::new();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
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
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_string(),
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
