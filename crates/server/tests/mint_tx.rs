use godcoin::{
    crypto::Signature,
    prelude::{script::EvalErrType, *},
};

mod common;
pub use common::*;

#[test]
fn mint_tx_verification() {
    let minter = TestMinter::new();
    let chain = minter.chain();
    let skip_flags = blockchain::skip_flags::SKIP_NONE;

    let create_tx = |fee: &str| {
        let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
            base: create_tx_header(fee),
            to: minter.genesis_info().owner_id,
            amount: Asset::default(),
            attachment: vec![],
            attachment_name: "".to_owned(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let tx = create_tx("0.00000 TEST");
    assert_eq!(
        chain.execute_tx(&tx.precompute(), &[], skip_flags),
        Ok(vec![])
    );

    let tx = create_tx("1.00000 TEST");
    assert_eq!(
        chain
            .execute_tx(&tx.precompute(), &[], skip_flags)
            .unwrap_err(),
        blockchain::TxErr::InvalidFeeAmount
    );

    let mut tx = create_tx("0.00000 TEST");
    tx.sigs_mut().remove(1);
    assert!(check_sigs(&tx));
    match chain.execute_tx(&tx.precompute(), &[], skip_flags) {
        Err(blockchain::TxErr::ScriptEval(e)) => assert_eq!(e.err, EvalErrType::ScriptRetFalse),
        res @ _ => panic!("Assertion failed, got {:?}", res),
    }

    let mut tx = create_tx("0.00000 TEST");
    tx.sigs_mut().clear();
    assert!(check_sigs(&tx));
    match chain.execute_tx(&tx.precompute(), &[], skip_flags) {
        Err(blockchain::TxErr::ScriptEval(e)) => assert_eq!(e.err, EvalErrType::ScriptRetFalse),
        res @ _ => panic!("Assertion failed, got {:?}", res),
    }

    let mut tx = create_tx("0.00000 TEST");
    tx.sigs_mut().clear();
    tx.sigs_mut().push(SigPair {
        pub_key: minter.genesis_info().wallet_keys[0].0.clone(),
        signature: Signature::from_slice(&[0; 64]).unwrap(),
    });
    assert!(!check_sigs(&tx));
    match chain.execute_tx(&tx.precompute(), &[], skip_flags) {
        Err(blockchain::TxErr::ScriptEval(e)) => assert_eq!(e.err, EvalErrType::ScriptRetFalse),
        res @ _ => panic!("Assertion failed, got {:?}", res),
    }
}

#[test]
fn mint_tx_updates_balances() {
    let minter = TestMinter::new();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: minter.genesis_info().owner_id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
    }));

    tx.append_sign(&minter.genesis_info().wallet_keys[1]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let props = chain.get_properties();
    // The test blockchain comes preminted with tokens
    let expected_bal = get_asset("1010.00000 TEST");
    assert_eq!(props.token_supply, expected_bal);

    let bal = chain
        .get_account(minter.genesis_info().owner_id, &[])
        .unwrap()
        .balance;
    assert_eq!(bal, expected_bal);
}
