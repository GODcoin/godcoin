use godcoin::crypto::{KeyPair, PrivateKey, Wif};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, DB};
use sodiumoxide::{
    crypto::{
        pwhash::argon2id13,
        secretbox::{self, gen_key, Key},
    },
    randombytes::randombytes_into,
};
use std::{borrow::Cow, path::PathBuf};

mod crypto;

use self::crypto::*;

pub const CF_ACCOUNTS: &str = "accounts";

pub const PROP_INIT: &[u8] = b"init";

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

    pub fn set_password(&mut self, pass: &[u8]) {
        use self::DbState::*;
        assert!(
            self.state == New || self.state == Unlocked,
            "invalid state setting password"
        );

        let salt = {
            let mut bytes = [0; argon2id13::SALTBYTES];
            randombytes_into(&mut bytes);
            argon2id13::Salt(bytes)
        };

        let perm_key = {
            let temp_key = {
                let mut bytes = [0; secretbox::KEYBYTES];
                argon2id13::derive_key(
                    &mut bytes,
                    &pass,
                    &salt,
                    argon2id13::OPSLIMIT_MODERATE,
                    argon2id13::MEMLIMIT_MODERATE,
                )
                .unwrap();
                Key(bytes)
            };

            let perm_key_dec = match self.state {
                DbState::New => Cow::Owned(gen_key()),
                DbState::Unlocked => Cow::Borrowed(self.key.as_ref().unwrap()),
                _ => unreachable!(),
            };

            encrypt_with_key(perm_key_dec.as_ref().as_ref(), &temp_key)
        };

        let mut prop_bytes = Vec::with_capacity(salt.as_ref().len() + perm_key.len());
        prop_bytes.extend_from_slice(salt.as_ref());
        prop_bytes.extend_from_slice(&perm_key);
        self.db.put(PROP_INIT, prop_bytes).unwrap();

        // If the wallet is already unlocked, lock it again.
        self.lock();
    }

    pub fn unlock(&mut self, pass: &[u8]) -> bool {
        assert_eq!(self.state, DbState::Locked);

        let init_bytes = self.db.get(PROP_INIT).unwrap().unwrap();
        let salt = argon2id13::Salt::from_slice(&init_bytes[0..argon2id13::SALTBYTES]).unwrap();

        let temp_key = {
            let mut bytes = [0; secretbox::KEYBYTES];
            argon2id13::derive_key(
                &mut bytes,
                &pass,
                &salt,
                argon2id13::OPSLIMIT_MODERATE,
                argon2id13::MEMLIMIT_MODERATE,
            )
            .unwrap();
            Key(bytes)
        };

        let key_enc = &init_bytes[argon2id13::SALTBYTES..];
        match decrypt_with_key(&key_enc, &temp_key) {
            Some(mut key) => {
                self.key = Some(Key::from_slice(&key).unwrap());
                self.state = DbState::Unlocked;
                sodiumoxide::utils::memzero(&mut key);

                true
            }
            None => false,
        }
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
