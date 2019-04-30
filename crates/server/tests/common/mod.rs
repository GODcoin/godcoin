use godcoin::prelude::*;
use godcoin_server::prelude::*;
use sodiumoxide::randombytes;
use std::{
    env, fs,
    ops::{Deref, DerefMut, Drop},
    path::PathBuf,
    sync::Arc,
};

pub struct TestMinter(Minter, Arc<Blockchain>, PathBuf);

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
        let minter = Minter::new(Arc::clone(&chain), minter_key, minter_wallet);
        Self(minter, chain, tmp_dir)
    }

    pub fn chain(&self) -> &Blockchain {
        &self.1
    }
}

impl Drop for TestMinter {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.2).expect("Failed to rm dir");
    }
}

impl Deref for TestMinter {
    type Target = Minter;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TestMinter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
