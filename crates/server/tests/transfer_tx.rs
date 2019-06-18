use actix::prelude::*;
use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn basic_transfer() {
    System::run(|| {
        let minter = TestMinter::new();
        let to_addr = KeyPair::gen();

        let create_tx = |fee: &str, amount: Asset| {
            let mut tx = TransferTx {
                base: create_tx_header(TxType::TRANSFER, fee),
                from: (&minter.genesis_info().script).into(),
                to: (&to_addr.0).into(),
                amount,
                memo: vec![],
                script: minter.genesis_info().script.clone(),
            };
            tx.append_sign(&minter.genesis_info().wallet_keys[3]);
            tx.append_sign(&minter.genesis_info().wallet_keys[0]);
            TxVariant::TransferTx(tx)
        };

        let bal = get_asset("1.0000 GRAEL");
        let tx = create_tx("1.0000 GRAEL", bal);
        let fut = minter.request(MsgRequest::Broadcast(tx));
        System::current().arbiter().send(
            fut.and_then(move |res| {
                assert_eq!(res, MsgResponse::Broadcast());
                minter.produce_block().map(|_| minter)
            })
            .and_then(move |minter| {
                let chain = minter.chain();
                let cur_bal = chain.get_balance(&to_addr.0.into(), &[]);
                assert_eq!(cur_bal, Some(bal));

                System::current().stop();
                Ok(())
            }),
        );
    })
    .unwrap();
}
