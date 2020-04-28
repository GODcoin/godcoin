use godcoin::{blockchain::index::WriteBatch, constants::*, prelude::*};

mod common;
pub use common::*;

#[test]
fn update_acc_tx_broadcast_success_change_script() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    let req_fee = owner_info
        .total_fee()
        .unwrap()
        .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
        .unwrap();

    let script = Script::new(vec![0x00, 0x01, 0x02]);
    let update_acc_tx = {
        let mut tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
            base: create_tx_header(&req_fee.to_string()),
            account_id: owner_id,
            new_script: Some(script.clone()),
            new_permissions: None,
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter.send_req(rpc::Request::Broadcast(update_acc_tx.clone()));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));

    // Assert changes take effect immediately before a block is produced
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    assert_eq!(owner_info.account.script, script);

    // Assert changes are still in effect after a block is produced
    minter.produce_block().unwrap();
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    assert_eq!(owner_info.account.script, script);
}

#[test]
fn update_acc_tx_broadcast_success_change_perms() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    let req_fee = owner_info
        .total_fee()
        .unwrap()
        .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
        .unwrap();

    let perms = Permissions {
        threshold: 0,
        keys: vec![],
    };
    let update_acc_tx = {
        let mut tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
            base: create_tx_header(&req_fee.to_string()),
            account_id: owner_id,
            new_script: None,
            new_permissions: Some(perms.clone()),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter.send_req(rpc::Request::Broadcast(update_acc_tx.clone()));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));

    // Assert changes take effect immediately before a block is produced
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    assert_eq!(owner_info.account.permissions, perms);

    // Assert changes are still in effect after a block is produced
    minter.produce_block().unwrap();
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    assert_eq!(owner_info.account.permissions, perms);
}

#[test]
fn update_acc_tx_fail_sig_threshold() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    let req_fee = owner_info
        .total_fee()
        .unwrap()
        .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
        .unwrap();

    let update_acc_tx = {
        let mut tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
            base: create_tx_header(&req_fee.to_string()),
            account_id: owner_id,
            new_script: None,
            new_permissions: None,
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };

    let res = minter.send_req(rpc::Request::Broadcast(update_acc_tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::ScriptEval(script::EvalErr {
                pos: 0,
                err: script::EvalErrKind::ScriptRetFalse
            })
        )))
    );
}

#[test]
fn update_acc_tx_valid_permissions() {
    let minter = TestMinter::new();

    let expected_res = Some(Err(net::ErrorKind::TxValidation(
        blockchain::TxErr::InvalidAccountPermissions,
    )));

    let fail_update_acc = |perms: Permissions| {
        let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
            base: create_tx_header("1.00000 TEST"),
            account_id: minter.genesis_info().owner_id,
            new_script: None,
            new_permissions: Some(perms),
        }));

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(res, expected_res);
    };

    // Test threshold cannot exceed the key count
    fail_update_acc(Permissions {
        threshold: 2,
        keys: vec![KeyPair::gen().0],
    });

    // Test keys cannot exceed MAX_PERM_KEYS
    fail_update_acc(Permissions {
        threshold: 1,
        keys: (0..=MAX_PERM_KEYS).map(|_| KeyPair::gen().0).collect(),
    });

    // Test immutable account has no keys
    fail_update_acc(Permissions {
        threshold: IMMUTABLE_ACCOUNT_THRESHOLD,
        keys: vec![KeyPair::gen().0],
    });
}

#[test]
fn update_acc_tx_fail_with_negative_amts() {
    let minter = TestMinter::new();

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: create_tx_header("-1.00000 TEST"),
        account_id: minter.genesis_info().owner_id,
        new_script: None,
        new_permissions: None,
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidFeeAmount,
        )))
    );
}

#[test]
fn update_acc_tx_fail_lessthan_min_fee() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.chain().get_account_info(owner_id, &[]);
    let req_fee = owner_info
        .unwrap()
        .total_fee()
        .unwrap()
        .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
        .unwrap();

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: create_tx_header(
            &req_fee
                .checked_sub(get_asset("0.00001 TEST"))
                .unwrap()
                .to_string(),
        ),
        account_id: minter.genesis_info().owner_id,
        new_script: None,
        new_permissions: None,
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidFeeAmount,
        )))
    );
}

#[test]
fn update_acc_tx_fail_insufficient_balance() {
    let minter = TestMinter::new();

    let owner_id = minter.genesis_info().owner_id;
    let owner_info = minter.minter().get_account_info(owner_id).unwrap();
    assert_eq!(owner_info.account.balance, get_asset("1000.00000 TEST"));

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: create_tx_header("1000.00001 TEST"),
        account_id: minter.genesis_info().owner_id,
        new_script: None,
        new_permissions: None,
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidAmount,
        )))
    );
}

#[test]
fn update_acc_tx_fail_script_too_large() {
    let minter = TestMinter::new();

    let script = Script::new(
        (0..=godcoin::constants::MAX_SCRIPT_BYTE_SIZE)
            .map(|_| 0u8)
            .collect(),
    );

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: create_tx_header("1.00000 TEST"),
        account_id: minter.genesis_info().owner_id,
        new_script: Some(script),
        new_permissions: None,
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
fn update_acc_tx_fail_attempt_update_destroyed_acc() {
    let minter = TestMinter::new();

    let account_id = 100;
    {
        let mut account = Account::create_default(
            account_id,
            Permissions {
                threshold: 0,
                keys: vec![],
            },
        );
        account.destroyed = true;

        let indexer = minter.chain().indexer();
        let mut batch = WriteBatch::new(indexer);
        batch.insert_or_update_account(account);
        batch.commit();
    }

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: create_tx_header("1.00000 TEST"),
        account_id,
        new_script: None,
        new_permissions: None,
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
fn update_acc_tx_fail_update_unknown_acc() {
    let minter = TestMinter::new();

    let tx = TxVariant::V0(TxVariantV0::UpdateAccountTx(UpdateAccountTx {
        base: create_tx_header("1.00000 TEST"),
        account_id: 0xFFFF,
        new_script: None,
        new_permissions: None,
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::AccountNotFound,
        )))
    );
}
