use actix::prelude::*;
use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn basic_transfer() {
    System::run(|| {
        let minter = TestMinter::new();
        let from_addr = ScriptHash::from(&minter.genesis_info().script);
        let to_addr = KeyPair::gen();

        let create_tx = |fee: &str, amount: Asset| {
            let mut tx = TransferTx {
                base: create_tx_header(TxType::TRANSFER, fee),
                from: from_addr.clone(),
                to: (&to_addr.0).into(),
                amount,
                memo: vec![],
                script: minter.genesis_info().script.clone(),
            };
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            TxVariant::TransferTx(tx)
        };

        let from_bal = minter.chain().get_balance(&from_addr, &[]).unwrap();
        let amount = get_asset("1.0000 GRAEL");
        let tx = create_tx("1.0000 GRAEL", amount);
        let fut = minter.request(MsgRequest::Broadcast(tx));
        System::current().arbiter().send(
            fut.and_then(move |res| {
                assert_eq!(res, MsgResponse::Broadcast());
                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let chain = minter.chain();
                let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
                assert_eq!(cur_bal, Some(amount));

                // The fee transfers back to the minter wallet in the form of a reward tx so it
                // must not be subtracted during the assertion
                let cur_bal = chain.get_balance(&from_addr, &[]);
                assert_eq!(cur_bal, from_bal.sub(amount));

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}
