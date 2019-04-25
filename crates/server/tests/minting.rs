use godcoin::test::*;

#[test]
fn empty_blockchain() {
    let blockchain = TestBlockchain::new();
    assert!(blockchain.get_block(0).is_none());
}
