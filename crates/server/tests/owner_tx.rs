use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn owner_tx_minter_key_change() {
    let minter = TestMinter::new();

    let minter_key = KeyPair::gen();
    let wallet_acc = {
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

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: create_tx_header("0.00000 TEST"),
            minter: minter_key.0,
            wallet: wallet_acc.id,
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
    assert_eq!(res.unwrap_err(), blockchain::BlockErr::InvalidSignature);
}

#[test]
fn owner_tx_deny_mint_tokens() {
    let minter = TestMinter::new();
    let wallet_acc = {
        let key = KeyPair::gen();
        let mut acc = Account::create_default(
            1,
            Permissions {
                threshold: 1,
                keys: vec![key.0.clone()],
            },
        );
        acc.script = script::Builder::new()
            .push(
                script::FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(1))
                    .push(OpFrame::OpCheckPerms),
            )
            .build()
            .unwrap();
        acc.balance = get_asset("4.00000 TEST");
        minter.create_account(acc, "2.00000 TEST", true)
    };

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: create_tx_header("0.00000 TEST"),
            minter: minter.genesis_info().minter_key.0.clone(),
            wallet: wallet_acc.id,
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
        to: wallet_acc.id,
        amount: get_asset("10.00000 TEST"),
        attachment: vec![],
        attachment_name: "".to_owned(),
    }));
    // Sign it again with the old keys, even though we changed the owner account.
    tx.append_sign(&minter.genesis_info().wallet_keys[3]);
    tx.append_sign(&minter.genesis_info().wallet_keys[0]);

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    match res {
        Err(net::ErrorKind::TxValidation(blockchain::TxErr::ScriptEval(script::EvalErr {
            err: script::EvalErrType::ScriptRetFalse,
            ..
        }))) => {}
        _ => panic!("Expected another response but got {:?}", res),
    }
}

#[test]
fn owner_tx_accept_mint_tokens() {
    let minter = TestMinter::new();

    let (wallet_acc, wallet_key) = {
        let key = KeyPair::gen();
        let mut acc = Account::create_default(
            1,
            Permissions {
                threshold: 1,
                keys: vec![key.0.clone()],
            },
        );
        acc.script = script::Builder::new()
            .push(
                script::FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::AccountId(1))
                    .push(OpFrame::OpCheckPerms),
            )
            .build()
            .unwrap();
        acc.balance = get_asset("4.00000 TEST");
        (minter.create_account(acc, "2.00000 TEST", true), key)
    };

    {
        // Assign the new owner wallet account
        let tx = {
            let mut tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
                base: create_tx_header("0.00000 TEST"),
                minter: minter.genesis_info().minter_key.0.clone(),
                wallet: wallet_acc.id,
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
    }
    {
        // Test minting tokens with the new wallet account
        let mut tx = TxVariant::V0(TxVariantV0::MintTx(MintTx {
            base: create_tx_header("0.00000 TEST"),
            to: wallet_acc.id,
            amount: get_asset("1000.00000 TEST"),
            attachment: vec![],
            attachment_name: "".to_owned(),
        }));
        tx.append_sign(&wallet_key);
        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
        minter.produce_block().unwrap();

        let chain = minter.chain();
        let props = chain.get_properties();
        let expected_bal = get_asset("2000.00000 TEST");
        assert_eq!(props.token_supply, expected_bal);

        // Add an additional 4 tokens since that is the starting balance of the new account.
        let bal = chain.get_account(wallet_acc.id, &[]).unwrap().balance;
        assert_eq!(bal, get_asset("1004.00000 TEST"));
    }
}
