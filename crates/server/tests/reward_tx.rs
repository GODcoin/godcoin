use actix::prelude::*;
use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn deny_broadcasted_reward_tx() {
    System::run(|| {
        let minter = TestMinter::new();

        let tx = TxVariant::RewardTx(RewardTx {
            base: create_tx_header(TxType::REWARD, "0.00000 GRAEL"),
            to: KeyPair::gen().0.into(),
            rewards: get_asset("1.00000 GRAEL"),
        });

        let fut = minter.request(MsgRequest::Broadcast(tx));
        Arbiter::spawn(fut.and_then(move |res| {
            assert!(res.is_err(), format!("{:?}", res));
            assert_eq!(
                res,
                MsgResponse::Error(net::ErrorKind::TxValidation(verify::TxErr::TxProhibited))
            );

            System::current().stop();
            Ok(())
        }));
    })
    .unwrap();
}
