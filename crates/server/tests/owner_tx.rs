use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn owner_tx_minter_key_change() {
    let minter = TestMinter::new();

    let minter_key = KeyPair::gen();
    let wallet_key = KeyPair::gen();

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: create_tx_header("0.00000 TEST"),
            minter: minter_key.0,
            wallet: wallet_key.0.into(),
            script: minter.genesis_info().script.clone(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));
    minter.produce_block().unwrap();

    let owner = minter.chain().get_owner();
    assert_eq!(tx, owner);

    // Minter key changed, should fail
    let res = minter.produce_block();
    assert_eq!(res.unwrap_err(), verify::BlockErr::InvalidSignature);
}

#[test]
fn owner_tx_deny_mint_tokens() {
    let minter = TestMinter::new();
    let wallet_key = KeyPair::gen();

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: create_tx_header("0.00000 TEST"),
            minter: minter.genesis_info().minter_key.0.clone(),
            wallet: (&wallet_key.0).into(),
            script: minter.genesis_info().script.clone(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));
    minter.produce_block().unwrap();

    let owner = minter.chain().get_owner();
    assert_eq!(tx, owner);
    minter.produce_block().unwrap();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: (&minter.genesis_info().script).into(),
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        // This is the old owner script, validation should fail
        script: minter.genesis_info().script.clone(),
    }));
    tx.append_sign(&wallet_key);

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            verify::TxErr::ScriptHashMismatch
        )))
    );
}

#[test]
fn owner_tx_accept_mint_tokens() {
    let minter = TestMinter::new();
    let wallet_key = KeyPair::gen();

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: create_tx_header("0.00000 TEST"),
            minter: minter.genesis_info().minter_key.0.clone(),
            wallet: (&wallet_key.0).into(),
            script: minter.genesis_info().script.clone(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter
        .send_req(rpc::Request::Broadcast(tx.clone()))
        .unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));
    minter.produce_block().unwrap();

    let owner = minter.chain().get_owner();
    assert_eq!(tx, owner);
    minter.produce_block().unwrap();

    let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
        base: create_tx_header("0.00000 TEST"),
        to: wallet_key.0.clone().into(),
        amount: get_asset("1000.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
        script: wallet_key.0.clone().into(),
    }));
    tx.append_sign(&wallet_key);
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let props = chain.get_properties();
    let expected_bal = get_asset("2000.00000 TEST");
    assert_eq!(props.token_supply, expected_bal);

    let bal = chain.get_balance(&wallet_key.0.clone().into(), &[]);
    assert_eq!(bal, Some(get_asset("1000.00000 TEST")));
}
