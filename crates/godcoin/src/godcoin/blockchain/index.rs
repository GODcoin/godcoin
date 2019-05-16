use rocksdb::{ColumnFamilyDescriptor, DBRecoveryMode, Options, DB};
use std::io::Cursor;
use std::mem;
use std::path::Path;

use crate::asset::Balance;
use crate::crypto::ScriptHash;
use crate::serializer::*;
use crate::tx::{OwnerTx, TxVariant};

const CF_BLOCK_BYTE_POS: &str = "block_byte_pos";
const CF_ADDR_BAL: &str = "address_balance";

const KEY_NET_OWNER: &[u8] = b"network_owner";
const KEY_CHAIN_HEIGHT: &[u8] = b"chain_height";
const KEY_TOKEN_SUPPLY: &[u8] = b"token_supply";

pub struct Indexer {
    db: DB,
}

impl Indexer {
    pub fn new(path: &Path) -> Indexer {
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        db_opts.set_wal_recovery_mode(DBRecoveryMode::AbsoluteConsistency);

        let col_families = vec![
            ColumnFamilyDescriptor::new(CF_BLOCK_BYTE_POS, Options::default()),
            ColumnFamilyDescriptor::new(CF_ADDR_BAL, Options::default()),
        ];
        let db = DB::open_cf_descriptors(&db_opts, path, col_families).unwrap();
        Indexer { db }
    }

    pub fn get_block_byte_pos(&self, height: u64) -> Option<u64> {
        let cf = self.db.cf_handle(CF_BLOCK_BYTE_POS).unwrap();
        let buf = self.db.get_cf(cf, height.to_be_bytes()).unwrap()?;

        Some(u64_from_buf!(buf))
    }

    pub fn set_block_byte_pos(&self, height: u64, pos: u64) {
        let key = height.to_be_bytes();
        let val = pos.to_be_bytes();

        let cf = self.db.cf_handle(CF_BLOCK_BYTE_POS).unwrap();
        self.db.put_cf(cf, &key, &val).unwrap();
    }

    pub fn get_chain_height(&self) -> u64 {
        match self.db.get(KEY_CHAIN_HEIGHT).unwrap() {
            Some(val) => u64_from_buf!(val),
            None => 0,
        }
    }

    pub fn set_chain_height(&self, height: u64) {
        self.db.put(KEY_CHAIN_HEIGHT, height.to_be_bytes()).unwrap();
    }

    pub fn get_owner(&self) -> Option<OwnerTx> {
        let tx_buf = self.db.get(KEY_NET_OWNER).unwrap()?;
        let cur = &mut Cursor::<&[u8]>::new(&tx_buf);
        let tx = TxVariant::deserialize_with_sigs(cur).unwrap();
        match tx {
            TxVariant::OwnerTx(owner) => Some(owner),
            _ => panic!("expected owner transaction"),
        }
    }

    pub fn set_owner(&self, owner: &OwnerTx) {
        let val = {
            let mut vec = Vec::with_capacity(4096);
            TxVariant::OwnerTx(owner.clone()).serialize_with_sigs(&mut vec);
            vec
        };

        self.db.put(KEY_NET_OWNER, &val).unwrap();
    }

    pub fn get_balance(&self, hash: &ScriptHash) -> Option<Balance> {
        let cf = self.db.cf_handle(CF_ADDR_BAL).unwrap();
        let bal_buf = self.db.get_cf(cf, hash.as_ref()).unwrap()?;
        let cur = &mut Cursor::<&[u8]>::new(&bal_buf);
        let bal = cur.take_balance().unwrap();
        Some(bal)
    }

    pub fn set_balance(&self, hash: &ScriptHash, bal: &Balance) {
        let cf = self.db.cf_handle(CF_ADDR_BAL).unwrap();
        let key = hash.as_ref();
        let val = {
            let mut vec = Vec::with_capacity(mem::size_of::<Balance>());
            vec.push_balance(bal);
            vec
        };
        self.db.put_cf(cf, key, &val).unwrap();
    }

    pub fn get_token_supply(&self) -> Balance {
        let bal_buf = self.db.get(KEY_TOKEN_SUPPLY).unwrap();
        match bal_buf {
            Some(bal_buf) => {
                let cur = &mut Cursor::<&[u8]>::new(&bal_buf);
                cur.take_balance().unwrap()
            }
            None => Balance::default(),
        }
    }

    pub fn set_token_supply(&self, bal: &Balance) {
        let val = {
            let mut vec = Vec::with_capacity(mem::size_of::<Balance>());
            vec.push_balance(bal);
            vec
        };
        self.db.put(KEY_TOKEN_SUPPLY, &val).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sodiumoxide::randombytes;
    use std::{env, fs, panic};

    #[test]
    fn test_get_block_pos() {
        run_test(|indexer| {
            indexer.set_block_byte_pos(1, 327);
            assert!(indexer.get_block_byte_pos(0).is_none());
            assert_eq!(indexer.get_block_byte_pos(1).unwrap(), 327);
        });
    }

    #[test]
    fn test_get_chain_height() {
        run_test(|indexer| {
            assert_eq!(indexer.get_chain_height(), 0);
            indexer.set_chain_height(42);
            assert_eq!(indexer.get_chain_height(), 42);
        });
    }

    fn run_test<F>(func: F)
    where
        F: FnOnce(Indexer) -> () + panic::UnwindSafe,
    {
        let mut tmp_dir = env::temp_dir();
        {
            let mut s = String::from("godcoin_test_");
            let mut num: [u8; 8] = [0; 8];
            randombytes::randombytes_into(&mut num);
            s.push_str(&format!("{}", u64::from_be_bytes(num)));
            tmp_dir.push(s);
        }
        fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));

        let result = panic::catch_unwind(|| {
            let indexer = Indexer::new(&tmp_dir);
            func(indexer);
        });

        fs::remove_dir_all(&tmp_dir).expect("Failed to rm dir");
        assert!(result.is_ok());
    }
}
