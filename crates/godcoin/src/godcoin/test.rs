use crate::prelude::*;
use sodiumoxide::randombytes;
use std::{
    env, fs,
    ops::{Deref, DerefMut, Drop},
    path::PathBuf,
};

pub struct TestBlockchain(Blockchain, PathBuf);

impl TestBlockchain {
    pub fn new() -> Self {
        crate::init().unwrap();
        let mut tmp_dir = env::temp_dir();
        {
            let mut s = String::from("godcoin_test_");
            let mut num: [u8; 8] = [0; 8];
            randombytes::randombytes_into(&mut num);
            s.push_str(&format!("{}", u64::from_be_bytes(num)));
            tmp_dir.push(s);
        }
        fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));

        let chain = Blockchain::new(&tmp_dir);
        Self(chain, tmp_dir)
    }
}

impl Drop for TestBlockchain {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.1).expect("Failed to rm dir");
    }
}

impl Deref for TestBlockchain {
    type Target = Blockchain;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TestBlockchain {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
