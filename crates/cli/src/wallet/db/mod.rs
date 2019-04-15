use godcoin::crypto::{double_sha256, KeyPair, PrivateKey, Wif};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, DB};
use sodiumoxide::crypto::secretbox::{gen_key, Key};
use std::path::PathBuf;

mod crypto;

use self::crypto::*;

pub use self::crypto::Password;

pub const CF_ACCOUNTS: &str = "accounts";

pub const PROP_INIT: &[u8] = b"init";
pub const PROP_INIT_KEY: &[u8] = b"init_key";

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DbState {
    New,
    Locked,
    Unlocked,
}

pub struct Db {
    state: DbState,
    db: DB,
    key: Option<Key>,
}

impl Db {
    pub fn new(path: PathBuf) -> Db {
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let col_families = vec![ColumnFamilyDescriptor::new(CF_ACCOUNTS, Options::default())];
        let db = DB::open_cf_descriptors(&db_opts, path, col_families).unwrap();
        let state = if db.get(PROP_INIT).unwrap().is_some() {
            DbState::Locked
        } else {
            DbState::New
        };

        Db {
            state,
            db,
            key: None,
        }
    }

    pub fn state(&self) -> DbState {
        self.state
    }

    pub fn set_password(&mut self, pass: &Password) {
        use self::DbState::*;
        assert!(
            self.state == New || self.state == Unlocked,
            "invalid state setting password"
        );
        let key = double_sha256(&pass.0);
        let key = Key::from_slice(key.as_ref()).unwrap();
        {
            let hash = double_sha256(&key.0);
            let enc = encrypt_with_key(hash.as_ref(), &key);
            {
                // Sanity decryption test
                let dec = decrypt_with_key(&enc, &key).unwrap();
                assert_eq!(dec, hash.as_ref());
            }
            self.db.put(PROP_INIT, &enc).unwrap();
        }

        match self.state {
            DbState::New => {
                let perm_key = gen_key();
                let enc = encrypt_with_key(&perm_key.0, &key);
                self.db.put(PROP_INIT_KEY, &enc).unwrap();
            }
            DbState::Unlocked => {
                let perm_key = self.db.get(PROP_INIT_KEY).unwrap().unwrap();
                let enc = encrypt_with_key(&perm_key, &key);
                self.db.put(PROP_INIT_KEY, &enc).unwrap();
            }
            _ => unreachable!(),
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
            match msg {
                Some(msg) => {
                    let hash = double_sha256(&key.0);
                    if hash.as_ref() != msg.as_slice() {
                        return false;
                    }
                }
                None => return false,
            }
        }
        {
            let msg = self.db.get(PROP_INIT_KEY).unwrap().unwrap();
            let mut msg = decrypt_with_key(&msg, &key).unwrap();
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

    pub fn get_accounts(&self) -> Vec<(String, KeyPair)> {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let mut accounts = Vec::with_capacity(64);

        let cf = self.db.cf_handle(CF_ACCOUNTS).unwrap();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start).unwrap();
        for (key, value) in iter {
            let dec_key = decrypt_with_key(&key, secret).unwrap();
            let dec_key = String::from_utf8(dec_key).unwrap();
            let dec_val = decrypt_with_key(&value, secret).unwrap();
            let dec_val = String::from_utf8(dec_val).unwrap();
            accounts.push((dec_key, PrivateKey::from_wif(&dec_val).unwrap()));
        }
        accounts
    }

    pub fn get_account(&self, account: &str) -> Option<KeyPair> {
        for (acc, pair) in self.get_accounts() {
            if acc == account {
                return Some(pair);
            }
        }
        None
    }

    pub fn set_account(&self, account: &str, key: &PrivateKey) {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let enc_key = encrypt_with_key(account.as_bytes(), secret);
        let enc_value = encrypt_with_key(key.to_wif().as_bytes(), secret);
        let cf = self.db.cf_handle(CF_ACCOUNTS).unwrap();
        self.db.put_cf(cf, &enc_key, &enc_value).unwrap();
    }

    pub fn del_account(&self, account: &str) -> bool {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let cf = self.db.cf_handle(CF_ACCOUNTS).unwrap();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start).unwrap();
        for (key, _) in iter {
            let dec_key = decrypt_with_key(&key, secret).unwrap();
            let dec_key = String::from_utf8(dec_key).unwrap();
            if dec_key == account {
                self.db.delete_cf(cf, &key).unwrap();
                return true;
            }
        }
        false
    }
}
