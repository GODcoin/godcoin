use godcoin::{constants::*, prelude::*};

mod common;
pub use common::*;

#[test]
fn create_acc_tx_broadcast_success() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.chain().get_account_info(owner_id, &[]).unwrap();
    let req_fee = owner_info
        .total_fee()
        .unwrap()
        .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
        .unwrap();
    let min_bal = req_fee.checked_mul(GRAEL_ACC_CREATE_MIN_BAL_MULT).unwrap();

    let create_acc_tx = {
        let mut account = Account::create_default(
            100,
            Permissions {
                threshold: 0,
                keys: vec![],
            },
        );
        account.balance = get_asset(&min_bal.to_string());

        let mut tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header(&req_fee.to_string()),
            creator: owner_id,
            account,
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter.send_req(rpc::Request::Broadcast(create_acc_tx));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
}

#[test]
fn create_acc_tx_test_creator_acc_threshold() {
    let minter = TestMinter::new();

    let owner_threshold = minter
        .chain()
        .get_account(minter.genesis_info().owner_id, &[])
        .unwrap()
        .permissions
        .threshold;

    let create_acc_tx = {
        let mut account = Account::create_default(
            100,
            Permissions {
                threshold: 0,
                keys: vec![],
            },
        );
        account.balance = get_asset("2.00000 TEST");

        let mut tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header("1.00000 TEST"),
            creator: minter.genesis_info().owner_id,
            account,
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        assert!(tx.sigs().len() < usize::from(owner_threshold));
        tx
    };

    let res = minter.send_req(rpc::Request::Broadcast(create_acc_tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::ScriptEval(script::EvalErr {
                pos: 0,
                err: script::EvalErrType::ScriptRetFalse
            })
        )))
    );
}

#[test]
fn create_acc_tx_test_new_acc_has_valid_permissions() {
    let minter = TestMinter::new();

    let expected_res = Some(Err(net::ErrorKind::TxValidation(
        blockchain::TxErr::InvalidAccountPermissions,
    )));

    let fail_create_acc = |perms: Permissions| {
        let mut account = Account::create_default(100, perms);
        account.balance = get_asset("2.00000 TEST");

        let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header("1.00000 TEST"),
            creator: minter.genesis_info().owner_id,
            account,
        }));

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(res, expected_res);
    };

    // Test threshold cannot exceed the key count
    fail_create_acc(Permissions {
        threshold: 2,
        keys: vec![KeyPair::gen().0],
    });

    // Test keys cannot exceed MAX_PERM_KEYS
    fail_create_acc(Permissions {
        threshold: 1,
        keys: (0..=MAX_PERM_KEYS).map(|_| KeyPair::gen().0).collect(),
    });

    // Test immutable account has no keys
    fail_create_acc(Permissions {
        threshold: IMMUTABLE_ACCOUNT_THRESHOLD,
        keys: vec![KeyPair::gen().0],
    });
}

#[test]
fn create_acc_tx_fail_with_negative_amts() {
    let minter = TestMinter::new();

    let perms = Permissions {
        threshold: 0,
        keys: vec![],
    };

    {
        let mut account = Account::create_default(100, perms.clone());
        account.balance = get_asset("2.00000 TEST");

        let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header("-1.00000 TEST"),
            creator: minter.genesis_info().owner_id,
            account,
        }));

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(
            res,
            Some(Err(net::ErrorKind::TxValidation(
                blockchain::TxErr::InvalidFeeAmount,
            )))
        );
    }

    {
        let mut account = Account::create_default(100, perms);
        account.balance = get_asset("-2.00000 TEST");

        let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header("1.00000 TEST"),
            creator: minter.genesis_info().owner_id,
            account,
        }));

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(
            res,
            Some(Err(net::ErrorKind::TxValidation(
                blockchain::TxErr::InvalidAmount,
            )))
        );
    }
}

#[test]
fn create_acc_tx_fail_script_too_large() {
    let minter = TestMinter::new();

    let mut account = Account::create_default(
        100,
        Permissions {
            threshold: 0,
            keys: vec![],
        },
    );
    account.balance = get_asset("2.00000 TEST");
    account.script = Script::new(
        (0..=godcoin::constants::MAX_SCRIPT_BYTE_SIZE)
            .map(|_| 0u8)
            .collect(),
    );

    let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
        base: create_tx_header("1.00000 TEST"),
        creator: minter.genesis_info().owner_id,
        account,
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::TxTooLarge,
        )))
    );
}

#[test]
fn create_acc_tx_fail_attempt_create_destroyed_acc() {
    let minter = TestMinter::new();

    let mut account = Account::create_default(
        100,
        Permissions {
            threshold: 0,
            keys: vec![],
        },
    );
    account.destroyed = true;

    let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
        base: create_tx_header("1.00000 TEST"),
        creator: minter.genesis_info().owner_id,
        account,
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::TxProhibited,
        )))
    );
}

#[test]
fn create_acc_tx_fail_lessthan_min_fee() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.chain().get_account_info(owner_id, &[]);
    let req_fee = owner_info
        .unwrap()
        .total_fee()
        .unwrap()
        .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
        .unwrap();
    let min_bal = req_fee.checked_mul(GRAEL_ACC_CREATE_MIN_BAL_MULT).unwrap();

    let mut account = Account::create_default(
        100,
        Permissions {
            threshold: 0,
            keys: vec![],
        },
    );

    {
        let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header(
                &req_fee
                    .checked_sub(get_asset("0.00001 TEST"))
                    .unwrap()
                    .to_string(),
            ),
            creator: minter.genesis_info().owner_id,
            account: account.clone(),
        }));

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(
            res,
            Some(Err(net::ErrorKind::TxValidation(
                blockchain::TxErr::InvalidFeeAmount,
            )))
        );
    }

    {
        account.balance = min_bal.checked_sub(get_asset("0.00001 TEST")).unwrap();

        let tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header(&req_fee.to_string()),
            creator: minter.genesis_info().owner_id,
            account,
        }));

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(
            res,
            Some(Err(net::ErrorKind::TxValidation(
                blockchain::TxErr::InvalidAmount,
            )))
        );
    }
}

#[test]
fn create_acc_tx_fail_insufficient_balance() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.chain().get_account_info(owner_id, &[]).unwrap();
    assert_eq!(owner_info.account.balance, get_asset("1000.00000 TEST"));

    let create_acc_tx = {
        let mut account = Account::create_default(
            100,
            Permissions {
                threshold: 0,
                keys: vec![],
            },
        );
        account.balance = get_asset("500.00001 TEST");

        TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header("500.00000 TEST"),
            creator: minter.genesis_info().owner_id,
            account,
        }))
    };

    let res = minter.send_req(rpc::Request::Broadcast(create_acc_tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidAmount,
        )))
    );
}

#[test]
fn create_acc_tx_fail_create_account_twice() {
    let minter = TestMinter::new();

    let expected_res = Some(Err(net::ErrorKind::TxValidation(
        blockchain::TxErr::AccountAlreadyExists,
    )));

    let create_acc_tx = |id: AccountId| {
        let mut account = Account::create_default(
            id,
            Permissions {
                threshold: 0,
                keys: vec![],
            },
        );
        account.balance = get_asset("2.00000 TEST");

        let mut tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: create_tx_header("1.00000 TEST"),
            creator: minter.genesis_info().owner_id,
            account,
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    {
        // Test attempting to recreate the owner account which is in the genesis block
        let acc = create_acc_tx(minter.genesis_info().owner_id);
        let res = minter.send_req(rpc::Request::Broadcast(acc));
        assert_eq!(res, expected_res);
    }

    {
        // Test attempting to create an account with the same ID in the same block
        let acc = create_acc_tx(100);
        let res = minter.send_req(rpc::Request::Broadcast(acc));
        assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));

        let acc = create_acc_tx(100);
        let res = minter.send_req(rpc::Request::Broadcast(acc));
        assert_eq!(res, expected_res);

        minter.produce_block().unwrap();
    }

    {
        let acc = create_acc_tx(101);
        let res = minter.send_req(rpc::Request::Broadcast(acc));
        assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));

        // Flush the tx pool so that we test the account is properly indexed
        minter.produce_block().unwrap();

        let acc = create_acc_tx(101);
        let res = minter.send_req(rpc::Request::Broadcast(acc));
        assert_eq!(res, expected_res);
    }
}
