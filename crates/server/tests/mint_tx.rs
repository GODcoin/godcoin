use godcoin::{crypto::Signature, prelude::*};

mod common;
pub use common::*;

#[test]
fn mint_tx_verification() {
    let minter = TestMinter::new();
    let chain = minter.chain();
    let skip_flags = verify::SKIP_NONE;

    let create_tx = |fee: &str| {
        let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
            base: create_tx_header(fee),
            to: (&minter.genesis_info().script).into(),
            amount: Asset::default(),
            attachment: vec![],
            attachment_name: "".to_owned(),
            script: minter.genesis_info().script.clone(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let tx = create_tx("0.00000 GRAEL");
    assert!(chain.verify_tx(&tx.precompute(), &[], skip_flags).is_ok());

    let tx = create_tx("1.00000 GRAEL");
    assert_eq!(
        chain
            .verify_tx(&tx.precompute(), &[], skip_flags)
            .unwrap_err(),
        verify::TxErr::InvalidFeeAmount
    );

    let mut tx = create_tx("0.00000 GRAEL");
    tx.sigs_mut().remove(1);
    assert!(check_sigs(&tx));
    assert_eq!(
        chain
            .verify_tx(&tx.precompute(), &[], skip_flags)
            .unwrap_err(),
        verify::TxErr::ScriptRetFalse
    );

    let mut tx = create_tx("0.00000 GRAEL");
    tx.sigs_mut().clear();
    assert!(check_sigs(&tx));
    assert_eq!(
        chain
            .verify_tx(&tx.precompute(), &[], skip_flags)
            .unwrap_err(),
        verify::TxErr::ScriptRetFalse
    );

    let mut tx = create_tx("0.00000 GRAEL");
    tx.sigs_mut().clear();
    tx.sigs_mut().push(SigPair {
        pub_key: minter.genesis_info().wallet_keys[0].0.clone(),
        signature: Signature::from_slice(&[0; 64]).unwrap(),
    });
    assert!(!check_sigs(&tx));
    assert_eq!(
        chain
            .verify_tx(&tx.precompute(), &[], skip_flags)
            .unwrap_err(),
        verify::TxErr::ScriptRetFalse
    );
}

#[test]
fn mint_tx_updates_balances() {
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
    assert_eq!(res, Some(ResponseBody::Broadcast));
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let props = chain.get_properties();
    // The test blockchain comes preminted with tokens
    let expected_bal = get_asset("1010.00000 GRAEL");
    assert_eq!(props.token_supply, expected_bal);

    let bal = chain.get_balance(&(&minter.genesis_info().script).into(), &[]);
    assert_eq!(bal, Some(expected_bal));
}
