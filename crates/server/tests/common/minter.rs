use super::create_tx_header;
use actix::prelude::*;
use actix_web::{
    dev::{Body, ResponseBody},
    web,
};
use godcoin::{blockchain::GenesisBlockInfo, prelude::*};
use godcoin_server::{index, prelude::*, ServerData};
use sodiumoxide::randombytes;
use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

type Indexed = bool;

pub struct TestMinter(ServerData, GenesisBlockInfo, PathBuf, Indexed);

impl TestMinter {
    pub fn new() -> Self {
        godcoin::init().unwrap();
        let tmp_dir = {
            let mut tmp_dir = env::temp_dir();
            let mut num: [u8; 8] = [0; 8];
            randombytes::randombytes_into(&mut num);
            tmp_dir.push(&format!("godcoin_test_{}", u64::from_be_bytes(num)));
            tmp_dir
        };
        fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));

        let blocklog_loc = &Path::join(&tmp_dir, "blklog");
        let index_loc = &Path::join(&tmp_dir, "index");
        let chain = Arc::new(Blockchain::new(blocklog_loc, index_loc));
        let minter_key = KeyPair::gen();
        let info = chain.create_genesis_block(minter_key.clone());

        {
            let txs = {
                let mut txs = Vec::with_capacity(1);

                let mut tx = MintTx {
                    base: create_tx_header(TxType::MINT, "0.00000 GRAEL"),
                    to: (&info.script).into(),
                    amount: "1000.00000 GRAEL".parse().unwrap(),
                    attachment: vec![1, 2, 3],
                    attachment_name: "".to_owned(),
                    script: info.script.clone(),
                };

                tx.append_sign(&info.wallet_keys[1]);
                tx.append_sign(&info.wallet_keys[0]);

                let tx = TxVariant::MintTx(tx);
                txs.push(tx);

                txs.push(TxVariant::RewardTx(RewardTx {
                    base: Tx {
                        tx_type: TxType::REWARD,
                        fee: "0.00000 GRAEL".parse().unwrap(),
                        timestamp: 0,
                        signature_pairs: Vec::new(),
                    },
                    to: (&info.script).into(),
                    rewards: Asset::default(),
                }));
                txs
            };

            let head = chain.get_chain_head();
            let child = head.new_child(txs).sign(&info.minter_key);
            chain.insert_block(child).unwrap();
        }

        let minter = Minter::new(Arc::clone(&chain), minter_key).start();
        let data = ServerData { chain, minter };
        Self(data, info, tmp_dir, true)
    }

    pub fn unindexed(&mut self) {
        let unindexed_path = {
            let mut unindexed_path = self.2.clone();
            let mut num: [u8; 8] = [0; 8];
            randombytes::randombytes_into(&mut num);
            unindexed_path.push(&format!("unindexed_{}", u64::from_be_bytes(num)));
            unindexed_path
        };
        fs::create_dir(&unindexed_path)
            .expect(&format!("Could not create temp dir {:?}", &unindexed_path));
        fs::copy(self.2.join("blklog"), unindexed_path.join("blklog"))
            .expect("Could not copy block log");

        let blocklog_loc = &Path::join(&unindexed_path, "blklog");
        let index_loc = &Path::join(&unindexed_path, "index");
        self.0.chain = Arc::new(Blockchain::new(blocklog_loc, index_loc));
        self.3 = false;
    }

    pub fn reindex(&mut self) {
        let chain = Arc::clone(&self.0.chain);
        assert_eq!(chain.index_status(), IndexStatus::None);
        chain.reindex();
        self.0.minter = Minter::new(chain, self.1.minter_key.clone()).start();
        self.3 = true;
    }

    pub fn chain(&self) -> &Blockchain {
        &self.0.chain
    }

    pub fn genesis_info(&self) -> &GenesisBlockInfo {
        &self.1
    }

    pub fn produce_block(&self) -> impl Future<Item = Result<(), verify::BlockErr>, Error = ()> {
        self.0
            .minter
            .send(ForceProduceBlock)
            .map_err(|e| panic!("{}", e))
    }

    pub fn request(&self, req: MsgRequest) -> impl Future<Item = MsgResponse, Error = ()> {
        self.send_request(net::RequestType::Single(req))
            .map(|res| res.unwrap_single())
    }

    pub fn batch_request(
        &self,
        reqs: Vec<MsgRequest>,
    ) -> impl Future<Item = Vec<MsgResponse>, Error = ()> {
        self.send_request(net::RequestType::Batch(reqs))
            .map(|res| res.unwrap_batch())
    }

    pub fn send_request(
        &self,
        req: net::RequestType,
    ) -> impl Future<Item = net::ResponseType, Error = ()> {
        let mut buf = Vec::with_capacity(1_048_576);
        req.serialize(&mut buf);
        self.raw_request(buf)
    }

    pub fn raw_request(&self, bytes: Vec<u8>) -> impl Future<Item = net::ResponseType, Error = ()> {
        assert!(
            self.3,
            "attempting to send a request to an unindexed minter"
        );
        let buf = bytes::Bytes::from(bytes);
        index(web::Data::new(self.0.clone()), buf).map(|res| {
            let body = match res.body() {
                ResponseBody::Body(body) => body,
                ResponseBody::Other(body) => body,
            };
            let buf = match body {
                Body::Bytes(bytes) => bytes,
                _ => panic!("Expected bytes body: {:?}", body),
            };
            net::ResponseType::deserialize(&mut Cursor::new(buf)).unwrap()
        })
    }
}

impl Drop for TestMinter {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.2).expect("Failed to rm dir");
    }
}
