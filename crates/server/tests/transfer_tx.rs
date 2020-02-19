use godcoin::{constants::*, prelude::*};
use std::sync::Arc;

mod common;
pub use common::*;

#[test]
fn transfer_from_minter() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let from_bal = minter.chain().get_balance(&from_addr, &[]).unwrap();
    let to_addr = KeyPair::gen();
    let amount = get_asset("1.00000 TEST");

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 1,
            args: {
                let mut args = vec![];
                args.push_scripthash(&(&to_addr.0).into());
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
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(amount));

    // The fee transfers back to the minter wallet in the form of a reward tx so it
    // must not be subtracted during the assertion
    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, from_bal.checked_sub(amount));
}

#[test]
fn transfer_from_user() {
    let minter = TestMinter::new();

    let user_1_addr = KeyPair::gen();
    let user_2_addr = KeyPair::gen();

    let res = {
        let tx = {
            let amount = get_asset("100.00000 TEST");
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: create_tx_header("1.00000 TEST"),
                from: ScriptHash::from(&minter.genesis_info().script),
                script: minter.genesis_info().script.clone(),
                call_fn: 1,
                args: {
                    let mut args = vec![];
                    args.push_scripthash(&(&user_1_addr.0).into());
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
        minter.send_req(rpc::Request::Broadcast(tx))
    };
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));

    let tx = {
        let amount = get_asset("99.00000 TEST");
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: (&user_1_addr.0).into(),
            script: user_1_addr.0.clone().into(),
            call_fn: 0,
            args: {
                let mut args = vec![];
                args.push_scripthash(&(&user_2_addr.0).into());
                args.push_asset(amount);
                args
            },
            amount,
            memo: vec![],
        }));
        tx.append_sign(&user_1_addr);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
    minter.produce_block().unwrap();

    let user_1_bal = minter.chain().get_balance(&user_1_addr.0.into(), &[]);
    assert_eq!(user_1_bal, Some(get_asset("0.00000 TEST")));

    let user_2_bal = minter.chain().get_balance(&user_2_addr.0.into(), &[]);
    assert_eq!(user_2_bal, Some(get_asset("99.00000 TEST")));

    let minter_addr = ScriptHash::from(&minter.genesis_info().script);
    let minter_bal = minter.chain().get_balance(&minter_addr, &[]);
    // The fee loops back to the minter in a reward transaction generated by the
    // minter block production
    assert_eq!(minter_bal, Some(get_asset("901.00000 TEST")));
}

#[test]
fn invalid_fee_amt_caused_by_insufficient_balance() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let to_addr = KeyPair::gen();
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1001.00000 TEST"),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 0,
            args: vec![],
            amount: get_asset("0.00000 TEST"),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidAmount
        )))
    );
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(get_asset("0.00000 TEST")));

    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, Some(get_asset("1000.00000 TEST")));
}

#[test]
fn insufficient_fee() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let info = minter.chain().get_address_info(&from_addr, &[]).unwrap();
    let bad_fee = info
        .total_fee()
        .unwrap()
        .checked_sub(get_asset("0.00001 TEST"))
        .unwrap();
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header(&bad_fee.to_string()),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 0,
            args: vec![],
            amount: get_asset("0.00000 TEST"),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidFeeAmount
        )))
    );
}

#[test]
fn negative_fee_should_fail() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("-100.00000 TEST"),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 0,
            args: vec![],
            amount: get_asset("0.00000 TEST"),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidFeeAmount
        )))
    );
}

#[test]
fn invalid_amt_caused_by_insufficient_balance() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let to_addr = KeyPair::gen();
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 0,
            args: vec![],
            amount: get_asset("500000.00000 TEST"),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidAmount
        )))
    );
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(get_asset("0.00000 TEST")));

    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, Some(get_asset("1000.00000 TEST")));
}

#[test]
fn invalid_amt_caused_by_negative_amt() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let to_addr = KeyPair::gen();
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 0,
            args: vec![],
            amount: get_asset("-500000.00000 TEST"),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidAmount
        )))
    );
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(get_asset("0.00000 TEST")));

    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, Some(get_asset("1000.00000 TEST")));
}

#[test]
fn memo_too_large() {
    let minter = TestMinter::new();

    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let to_addr = KeyPair::gen();
    let tx = {
        let amount = get_asset("1.00000 TEST");
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 1,
            args: {
                let mut args = vec![];
                args.push_scripthash(&(&to_addr.0).into());
                args.push_asset(amount);
                args
            },
            amount,
            memo: (0..=godcoin::constants::MAX_MEMO_BYTE_SIZE)
                .map(|_| 0)
                .collect(),
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::TxTooLarge
        )))
    );
    minter.produce_block().unwrap();

    let chain = minter.chain();
    let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
    assert_eq!(cur_bal, Some(get_asset("0.00000 TEST")));

    let cur_bal = chain.get_balance(&from_addr, &[]);
    assert_eq!(cur_bal, Some(get_asset("1000.00000 TEST")));
}

#[test]
fn script_too_large() {
    let minter = TestMinter::new();

    let from_script = Script::new(
        (0..=godcoin::constants::MAX_SCRIPT_BYTE_SIZE)
            .map(|_| 0)
            .collect(),
    );
    let from_addr = ScriptHash::from(&from_script);
    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header("1.00000 TEST"),
            from: from_addr,
            script: from_script,
            call_fn: 0,
            args: vec![],
            amount: get_asset("1.00000 TEST"),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx));
    assert_eq!(
        res,
        Some(Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::TxTooLarge
        )))
    );
}

#[test]
fn tx_addr_dynamic_fee_increase_in_pool() {
    let minter = TestMinter::new();
    let from_addr = ScriptHash::from(&minter.genesis_info().script);

    let res = minter
        .send_req(rpc::Request::GetAddressInfo(from_addr.clone()))
        .unwrap();
    let addr_info = match res {
        Ok(rpc::Response::GetAddressInfo(info)) => info,
        unexp @ _ => panic!("Expected GetAddressInfo response: {:?}", unexp),
    };

    let tx = {
        let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: create_tx_header(&addr_info.total_fee().unwrap().to_string()),
            from: from_addr.clone(),
            script: minter.genesis_info().script.clone(),
            call_fn: 0,
            args: vec![],
            amount: Asset::new(0),
            memo: vec![],
        }));
        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);
        tx
    };
    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(res, Ok(rpc::Response::Broadcast));

    let res = minter
        .send_req(rpc::Request::GetAddressInfo(from_addr.clone()))
        .unwrap();
    let old_addr_info = addr_info;
    let addr_info = match res {
        Ok(rpc::Response::GetAddressInfo(info)) => info,
        unexp @ _ => panic!("Expected GetAddressInfo response: {:?}", unexp),
    };

    assert!(addr_info.addr_fee > old_addr_info.addr_fee);

    // Transaction count always start at 1, so test it as though two transactions
    // were made.
    let expected_fee = GRAEL_FEE_MIN
        .checked_mul(GRAEL_FEE_MULT.checked_pow(2).unwrap())
        .unwrap();
    assert_eq!(addr_info.addr_fee, expected_fee);

    minter.produce_block().unwrap();
    let res = minter
        .send_req(rpc::Request::GetAddressInfo(from_addr))
        .unwrap();
    let addr_info = match res {
        Ok(rpc::Response::GetAddressInfo(info)) => info,
        unexp @ _ => panic!("Expected GetAddressInfo response: {:?}", unexp),
    };
    assert_eq!(addr_info.addr_fee, expected_fee);
}

#[test]
fn tx_addr_dynamic_fee_increase() {
    let minter = Arc::new(TestMinter::new());
    let from_addr = ScriptHash::from(&minter.genesis_info().script);

    for num in 1..10 {
        let res = minter
            .send_req(rpc::Request::GetAddressInfo(from_addr.clone()))
            .unwrap();
        let addr_info = match res {
            Ok(rpc::Response::GetAddressInfo(info)) => info,
            unexp @ _ => panic!("Expected GetAddressInfo response: {:?}", unexp),
        };

        let expected_fee = GRAEL_FEE_MIN
            .checked_mul(GRAEL_FEE_MULT.checked_pow(num).unwrap())
            .unwrap();
        assert_eq!(addr_info.addr_fee, expected_fee);

        let tx = {
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: create_tx_header(&addr_info.total_fee().unwrap().to_string()),
                from: from_addr.clone(),
                script: minter.genesis_info().script.clone(),
                call_fn: 0,
                args: vec![],
                amount: Asset::new(0),
                memo: vec![],
            }));
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            tx
        };

        let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
        assert_eq!(res, Ok(rpc::Response::Broadcast));
        minter.produce_block().unwrap();
    }

    for _ in 0..=FEE_RESET_WINDOW {
        minter.produce_block().unwrap();
    }

    let res = minter
        .send_req(rpc::Request::GetAddressInfo(from_addr.clone()))
        .unwrap();
    let addr_info = match res {
        Ok(rpc::Response::GetAddressInfo(info)) => info,
        unexp @ _ => panic!("Expected GetAddressInfo response: {:?}", unexp),
    };

    // Test the delta reset for address fees
    let expected_fee = GRAEL_FEE_MIN.checked_mul(GRAEL_FEE_MULT).unwrap();
    assert_eq!(addr_info.addr_fee, expected_fee);
}

#[test]
fn net_fee_dynamic_increase() {
    let minter = Arc::new(TestMinter::new());
    let from_addr = ScriptHash::from(&minter.genesis_info().script);
    let addrs = Arc::new((0..100).map(|_| KeyPair::gen()).collect::<Vec<_>>());

    for addr_index in 0..addrs.len() {
        let tx = {
            let amount = Asset::new(100000);
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: create_tx_header("1.00000 TEST"),
                from: from_addr.clone(),
                script: minter.genesis_info().script.clone(),
                call_fn: 1,
                args: {
                    let mut args = vec![];
                    args.push_scripthash(&(&addrs[addr_index].0).into());
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

        let req = rpc::Request::Broadcast(tx);
        let res = minter.send_req(req.clone()).unwrap();
        let exp = Err(net::ErrorKind::TxValidation(
            blockchain::TxErr::InvalidFeeAmount,
        ));
        if res == exp {
            for _ in 0..=FEE_RESET_WINDOW {
                minter.produce_block().unwrap();
            }
            let res = minter.send_req(req).unwrap();
            assert_eq!(res, Ok(rpc::Response::Broadcast));
        } else {
            assert_eq!(res, Ok(rpc::Response::Broadcast));
        }
    }

    for addr_index in 0..addrs.len() {
        let tx = {
            let addr = &addrs[addr_index];
            let mut tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: create_tx_header("1.00000 TEST"),
                from: (&addr.0).into(),
                script: addr.0.clone().into(),
                call_fn: 0,
                args: {
                    let mut args = vec![];
                    args.push_scripthash(&from_addr);
                    args.push_asset(Asset::default());
                    args
                },
                amount: Asset::default(),
                memo: vec![],
            }));
            tx.append_sign(&addr);
            tx
        };

        let res = minter.send_req(rpc::Request::Broadcast(tx));
        assert_eq!(res, Some(Ok(rpc::Response::Broadcast)));
    }

    // Ensure the network fee gets updated
    for _ in 0..5 {
        minter.produce_block().unwrap();
    }

    {
        let res = minter.send_req(rpc::Request::GetProperties).unwrap();
        let props = match res {
            Ok(rpc::Response::GetProperties(props)) => props,
            unexp @ _ => panic!("Expected GetProperties response: {:?}", unexp),
        };

        let chain = minter.chain();
        let max_height = props.height - (props.height % 5);
        let min_height = max_height - NETWORK_FEE_AVG_WINDOW;
        assert!(min_height < max_height);

        let receipt_count = (min_height..=max_height).fold(1u64, |receipt_count, height| {
            let block = chain.get_block(height).unwrap();
            receipt_count + block.receipts().len() as u64
        });
        let receipt_count = (receipt_count / NETWORK_FEE_AVG_WINDOW) as u16;
        assert!(receipt_count > 10);

        let fee = GRAEL_FEE_MIN.checked_mul(GRAEL_FEE_NET_MULT.checked_pow(receipt_count).unwrap());
        assert_eq!(Some(props.network_fee), fee);
    }

    for _ in 0..=NETWORK_FEE_AVG_WINDOW {
        minter.produce_block().unwrap();
    }

    // Test network delta fee reset
    let expected_fee = GRAEL_FEE_MIN.checked_mul(GRAEL_FEE_NET_MULT);
    assert_eq!(minter.chain().get_network_fee(), expected_fee);
}
