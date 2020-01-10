use godcoin::prelude::*;

mod common;
pub use common::*;

#[test]
fn deny_broadcasted_reward_tx() {
    let minter = TestMinter::new();

    let tx = TxVariant::V0(TxVariantV0::RewardTx(RewardTx {
        base: create_tx_header("0.00000 TEST"),
        to: KeyPair::gen().0.into(),
        rewards: get_asset("1.00000 TEST"),
    }));

    let res = minter.send_req(rpc::Request::Broadcast(tx)).unwrap();
    assert_eq!(
        res,
        Err(net::ErrorKind::TxValidation(verify::TxErr::TxProhibited))
    );
}
