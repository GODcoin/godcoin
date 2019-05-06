use actix::prelude::*;
use godcoin::prelude::*;
use godcoin_server::{handle_request, prelude::*, ServerData};
use sodiumoxide::randombytes;
use std::{env, fs, ops::Drop, path::PathBuf, sync::Arc};

pub struct TestMinter(ServerData, PathBuf);

impl TestMinter {
    pub fn new() -> Self {
        godcoin::init().unwrap();
        let mut tmp_dir = env::temp_dir();
        {
            let mut s = String::from("godcoin_test_");
            let mut num: [u8; 8] = [0; 8];
            randombytes::randombytes_into(&mut num);
            s.push_str(&format!("{}", u64::from_be_bytes(num)));
            tmp_dir.push(s);
        }
        fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));

        let chain = Arc::new(Blockchain::new(&tmp_dir));

        let minter_key = KeyPair::gen_keypair();
        let minter_wallet = (&minter_key.0).into();
        let minter = Minter::new(Arc::clone(&chain), minter_key, minter_wallet).start();
        let data = ServerData { chain, minter };
        Self(data, tmp_dir)
    }

    pub fn chain(&self) -> &Blockchain {
        &self.0.chain
    }

    pub fn request(&self, req: MsgRequest) -> MsgResponse {
        handle_request(&self.0, req)
    }
}

impl Drop for TestMinter {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.1).expect("Failed to rm dir");
    }
}
