use godcoin::{
    account::AccountId,
    crypto::{KeyPair, PrivateKey, Wif},
    serializer::*,
};
use rocksdb::{ColumnFamilyDescriptor, IteratorMode, Options, DB};
use sodiumoxide::{
    crypto::{
        pwhash::argon2id13,
        secretbox::{self, gen_key, Key},
    },
    randombytes::randombytes_into,
};
use std::{borrow::Cow, io::Cursor, path::PathBuf};

mod crypto;

use self::crypto::*;

pub const CF_ACCOUNTS: &str = "accounts";

pub const PROP_INIT: &[u8] = b"init";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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

    pub fn get_accounts(&self) -> Vec<(String, WalletAccount)> {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let mut accounts = Vec::with_capacity(64);

        let cf = self.db.cf_handle(CF_ACCOUNTS).unwrap();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start).unwrap();
        for (key, value) in iter {
            let dec_key = decrypt_with_key(&key, secret).unwrap();
            let dec_key = String::from_utf8(dec_key).unwrap();
            let dec_val = decrypt_with_key(&value, secret).unwrap();
            let dec_val = WalletAccount::deserialize(&mut Cursor::new(&dec_val));
            accounts.push((dec_key, dec_val));
        }
        accounts
    }

    pub fn get_account(&self, account: &str) -> Option<WalletAccount> {
        for (acc, pair) in self.get_accounts() {
            if acc == account {
                return Some(pair);
            }
        }
        None
    }

    pub fn set_account(&self, name: &str, account: WalletAccount) {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let enc_key = encrypt_with_key(name.as_bytes(), secret);
        let enc_value = encrypt_with_key(&account.serialize(), secret);
        let cf = self.db.cf_handle(CF_ACCOUNTS).unwrap();
        self.db.put_cf(cf, &enc_key, &enc_value).unwrap();
    }

    pub fn del_account(&self, name: &str) -> bool {
        let secret = self.key.as_ref().expect("wallet not unlocked");
        let cf = self.db.cf_handle(CF_ACCOUNTS).unwrap();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start).unwrap();
        for (key, _) in iter {
            let dec_key = decrypt_with_key(&key, secret).unwrap();
            let dec_key = String::from_utf8(dec_key).unwrap();
            if dec_key == name {
                self.db.delete_cf(cf, &key).unwrap();
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug)]
pub struct WalletAccount {
    pub id: AccountId,
    pub keys: Vec<KeyPair>,
}

impl WalletAccount {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push_u64(self.id);
        buf.push_u16(self.keys.len() as u16);
        for key in &self.keys {
            buf.push_bytes(key.1.to_wif().as_bytes());
        }
        buf
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> WalletAccount {
        let id = cur.take_u64().unwrap();
        let key_len = cur.take_u16().unwrap();
        let mut keys = Vec::with_capacity(usize::from(key_len));
        for _ in 0..key_len {
            let key = {
                let bytes = cur.take_bytes().unwrap();
                let wif = String::from_utf8(bytes).unwrap();
                PrivateKey::from_wif(&wif).unwrap()
            };
            keys.push(key);
        }
        WalletAccount { id, keys }
    }
}
