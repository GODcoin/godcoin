use rocksdb::{DB, Options, ColumnFamilyDescriptor};
use sodiumoxide::crypto::secretbox::{Key, gen_key};
use godcoin::crypto::double_sha256;
use std::path::PathBuf;

mod constants;
mod crypto;

use self::constants::*;
use self::crypto::*;

pub use self::crypto::Password;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DbState {
    New,
    Locked,
    Unlocked
}

pub struct Db {
    state: DbState,
    db: DB,
    key: Option<Key>
}

impl Db {
    pub fn new(path: PathBuf) -> Db {
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let col_families = vec![
            ColumnFamilyDescriptor::new(CF_ACCOUNTS, Options::default())
        ];
        let db = DB::open_cf_descriptors(&db_opts, path, col_families).unwrap();
        let state = if db.get(PROP_INIT).unwrap().is_some() {
            DbState::Locked
        } else {
            DbState::New
        };

        Db {
            state,
            db,
            key: None
        }
    }

    pub fn state(&self) -> DbState {
        self.state
    }

    pub fn set_password(&mut self, pass: &Password) {
        use self::DbState::*;
        assert!(self.state == New || self.state == Unlocked, "invalid state setting password");
        let key = double_sha256(&pass.0);
        let key = Key::from_slice(key.as_ref()).unwrap();
        {
            let hash = double_sha256(&key.0);
            let enc = encrypt_with_key(hash.as_ref(), &key);
            {
                // Sanity decryption test
                let dec = decrypt_with_key(&enc, &key);
                assert_eq!(dec, hash.as_ref());
            }
            self.db.put(PROP_INIT, &enc).unwrap();
        }

        match self.state {
            DbState::New => {
                let perm_key = gen_key();
                let enc = encrypt_with_key(&perm_key.0, &key);
                self.db.put(PROP_INIT_KEY, &enc).unwrap();
            },
            DbState::Unlocked => {
                let perm_key = self.db.get(PROP_INIT_KEY).unwrap().unwrap();
                let enc = encrypt_with_key(&perm_key, &key);
                self.db.put(PROP_INIT_KEY, &enc).unwrap();
            },
            _ => unreachable!()
        }
        self.lock();
    }

    pub fn unlock(&mut self, pass: &Password) -> bool {
        assert_eq!(self.state, DbState::Locked);
        let key = double_sha256(&pass.0);
        let key = Key::from_slice(key.as_ref()).unwrap();
        {
            let msg = self.db.get(PROP_INIT).unwrap().unwrap();
            let msg = decrypt_with_key(&msg, &key);
            let hash = double_sha256(&key.0);
            if hash.as_ref() != msg.as_slice() { return false }
        }
        {
            let msg = self.db.get(PROP_INIT_KEY).unwrap().unwrap();
            let mut msg = decrypt_with_key(&msg, &key);
            self.key = Some(Key::from_slice(&msg).unwrap());
            self.state = DbState::Unlocked;
            sodiumoxide::utils::memzero(&mut msg);
        }
        true
    }

    pub fn lock(&mut self) {
        self.state = DbState::Locked;
        self.key = None;
    }

    fn set(&self, key: &[u8], value: &[u8]) {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let enc_key = encrypt_with_key(key, secret);
        let enc_value = encrypt_with_key(value, secret);
        self.db.put(&enc_key, &enc_value).unwrap();
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let key = encrypt_with_key(key, secret);
        self.db.get(&key).unwrap().map(|bytes| {
            decrypt_with_key(&bytes, secret)
        })
    }
}
