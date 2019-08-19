use godcoin::{
    blockchain::index::TxManager,
    constants,
    prelude::{net::ErrorKind, verify::TxErr, *},
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
        script::Builder::new().push(OpFrame::False).build()
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
    let amount = get_asset("1.00000 GRAEL");

    // Create a tx we expect to be reindexed
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 GRAEL"),
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
        let res = minter.request(MsgRequest::Broadcast(tx.clone()));
        assert_eq!(res, MsgResponse::Broadcast);
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
        script::Builder::new().push(OpFrame::False).build()
    );
    assert_eq!(owner.wallet, (&minter.genesis_info().script).into());

    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(amount));

    // The fee transfers back to the minter wallet in the form of a reward tx so it
    // must not be subtracted during the assertion
    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, from_bal.checked_sub(amount));

    // Test to ensure that after a reindex the tx cannot be rebroadcasted
    let res = minter.request(MsgRequest::Broadcast(tx.clone()));

    assert_eq!(
        res,
        MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxDupe))
    );
}

#[test]
fn tx_dupe() {
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

    let res = minter.request(MsgRequest::Broadcast(tx.clone()));
    assert!(!res.is_err(), format!("{:?}", res));

    let res = minter.request(MsgRequest::Broadcast(tx));
    assert!(res.is_err());
    assert_eq!(
        res,
        MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxDupe))
    );
}

#[test]
fn tx_expired() {
    use godcoin::constants::TX_EXPIRY_TIME;

    let minter = TestMinter::new();
    let time = godcoin::get_epoch_ms();

    let tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header_with_ts("0.00000 GRAEL", time + TX_EXPIRY_TIME),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 GRAEL"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    let res = minter.request(MsgRequest::Broadcast(tx));
    assert!(res.is_err());
    assert_eq!(
        res,
        MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxExpired))
    );
}

#[test]
fn tx_far_in_the_future() {
    let minter = TestMinter::new();
    let time = godcoin::get_epoch_ms();

    let tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header_with_ts("0.00000 GRAEL", time + 4000),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 GRAEL"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: minter.genesis_info().script.clone(),
    }));

    let res = minter.request(MsgRequest::Broadcast(tx));
    assert!(res.is_err());
    assert_eq!(
        res,
        MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxExpired))
    );
}

#[test]
fn tx_script_too_large_err() {
    let minter = TestMinter::new();

    let tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 GRAEL"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 GRAEL"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: Script::new((0..=constants::MAX_SCRIPT_BYTE_SIZE).map(|_| 0).collect()),
    }));

    let res = minter.request(MsgRequest::Broadcast(tx));
    assert!(res.is_err());
    assert_eq!(
        res,
        MsgResponse::Error(ErrorKind::TxValidation(TxErr::TxTooLarge))
    );
}

#[test]
fn tx_too_many_signatures_err() {
    let minter = TestMinter::new();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 GRAEL"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 GRAEL"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: Script::new(vec![]),
    }));
    (0..=constants::MAX_TX_SIGNATURES).for_each(|_| tx.append_sign(&KeyPair::gen()));

    let res = minter.request(MsgRequest::Broadcast(tx));
    assert!(res.is_err());
    assert_eq!(
        res,
        MsgResponse::Error(ErrorKind::TxValidation(TxErr::TooManySignatures))
    );
}
