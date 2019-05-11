use actix::prelude::*;
use godcoin::prelude::*;

mod common;

pub use common::*;

#[test]
fn fresh_blockchain() {
    System::run(|| {
        let minter = TestMinter::new();
        let chain = minter.chain();
        assert!(chain.get_block(0).is_some());
        assert_eq!(chain.get_chain_height(), 0);

        let owner = chain.get_owner();
        assert_eq!(owner.minter, minter.genesis_info().minter_key.0);
        assert_eq!(
            owner.script,
            script::Builder::new().push(OpFrame::False).build()
        );
        assert_eq!(owner.wallet, (&minter.genesis_info().script).into());

        assert!(chain.get_block(1).is_none());
        System::current().stop();
    })
    .unwrap();
}

#[test]
fn mint_tx_verification() {
    System::run(|| {
        let minter = TestMinter::new();
        let chain = minter.chain();
        let config = verify::Config::strict();

        let mut tx = MintTx {
            base: create_tx(TxType::MINT, "0 GOLD"),
            to: (&minter.genesis_info().script).into(),
            amount: Balance::default(),
            script: minter.genesis_info().script.clone(),
        };

        tx.append_sign(&minter.genesis_info().wallet_keys[3]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);

        let mut tx = TxVariant::MintTx(tx);
        assert!(chain.verify_tx(&tx, &[], config).is_ok());

        tx.fee = "1 GOLD".parse().unwrap();
        assert!(chain.verify_tx(&tx, &[], config).is_err());

        System::current().stop();
    })
    .unwrap();
}

#[test]
fn mint_tx_updates_balances() {
    System::run(|| {
        let minter = TestMinter::new();

        let mut tx = MintTx {
            base: create_tx(TxType::MINT, "0 GOLD"),
            to: (&minter.genesis_info().script).into(),
            amount: get_balance("10.0 GOLD", "1000 SILVER"),
            script: minter.genesis_info().script.clone(),
        };

        tx.append_sign(&minter.genesis_info().wallet_keys[1]);
        tx.append_sign(&minter.genesis_info().wallet_keys[0]);

        let tx = TxVariant::MintTx(tx);
        let fut = minter.request(MsgRequest::Broadcast(tx));
        System::current().arbiter().send(
            fut.then(move |res| {
                let res = res.unwrap();
                assert!(!res.is_err(), format!("{:?}", res));

                minter.produce_block().map(|_| minter)
            })
            .then(|res| {
                let minter = res.unwrap();
                let chain = minter.chain();
                let props = chain.get_properties();
                let expected_bal = get_balance("10 GOLD", "1000 SILVER");
                assert_eq!(props.token_supply, expected_bal);

                let bal = chain.get_balance(&(&minter.genesis_info().script).into());
                assert_eq!(bal, expected_bal);

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}
