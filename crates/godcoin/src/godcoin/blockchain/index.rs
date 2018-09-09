use rocksdb::{DB, ColumnFamilyDescriptor, Options};
use std::path::Path;
use std::io::Cursor;
use std::mem;

use tx::{TxVariant, BondTx};
use crypto::PublicKey;
use asset::Balance;
use serializer::*;

const CF_BLOCK_BYTE_POS: &str = "block_byte_pos";
const CF_ADDR_BAL: &str = "address_balance";
const CF_BOND: &str = "bond";

const KEY_CHAIN_HEIGHT: &[u8] = b"chain_height";

pub struct Indexer {
    db: DB
}

impl Indexer {
    pub fn new(path: &Path) -> Indexer {
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);

        let col_families = vec![
            ColumnFamilyDescriptor::new(CF_BLOCK_BYTE_POS, Options::default()),
            ColumnFamilyDescriptor::new(CF_ADDR_BAL, Options::default()),
            ColumnFamilyDescriptor::new(CF_BOND, Options::default())
        ];
        let db = DB::open_cf_descriptors(&db_opts, path, col_families).unwrap();
        Indexer { db }
    }

    pub fn get_block_byte_pos(&self, height: u64) -> Option<u64> {
        let cf = self.db.cf_handle(CF_BLOCK_BYTE_POS).unwrap();
        let buf = self.db.get_cf(cf, &{
            let mut key = Vec::with_capacity(8);
            key.push_u64(height);
            key
        }).unwrap()?;

        Some(u64_from_buf!(buf))
    }

    pub fn set_block_byte_pos(&self, height: u64, pos: u64) {
        let mut key = Vec::with_capacity(8);
        key.push_u64(height);
        let mut val = Vec::with_capacity(8);
        val.push_u64(pos);

        let cf = self.db.cf_handle(CF_BLOCK_BYTE_POS).unwrap();
        self.db.put_cf(cf, &key, &val).unwrap();
    }

    pub fn get_chain_height(&self) -> u64 {
        match self.db.get(KEY_CHAIN_HEIGHT).unwrap() {
            Some(val) => u64_from_buf!(val),
            None => 0
        }
    }

    pub fn set_chain_height(&self, height: u64) {
        let mut val = Vec::with_capacity(8);
        val.push_u64(height);
        self.db.put(KEY_CHAIN_HEIGHT, &val).unwrap();
    }

    pub fn get_bond(&self, minter: &PublicKey) -> Option<BondTx> {
        let cf = self.db.cf_handle(CF_BOND).unwrap();
        let tx_buf = self.db.get_cf(cf, minter.as_bytes()).unwrap()?;
        let cur = &mut Cursor::<&[u8]>::new(&tx_buf);
        let tx = TxVariant::decode_with_sigs(cur).unwrap();
        match tx {
            TxVariant::BondTx(bond) => Some(bond),
            _ => panic!("expected bond transaction")
        }
    }

    pub fn set_bond(&self, bond: &BondTx) {
        let cf = self.db.cf_handle(CF_BOND).unwrap();
        let key = bond.minter.as_bytes();
        let val = {
            let mut vec = Vec::with_capacity(2048);
            TxVariant::BondTx(bond.clone()).encode_with_sigs(&mut vec);
            vec
        };

        self.db.put_cf(cf, key, &val).unwrap();
    }

    pub fn get_balance(&self, addr: &PublicKey) -> Option<Balance> {
        let cf = self.db.cf_handle(CF_ADDR_BAL).unwrap();
        let bal_buf = self.db.get_cf(cf, addr.as_bytes()).unwrap()?;
        let cur = &mut Cursor::<&[u8]>::new(&bal_buf);
        let gold = cur.take_asset().unwrap();
        let silver = cur.take_asset().unwrap();
        Some(Balance { gold, silver })
    }

    pub fn set_balance(&self, addr: &PublicKey, bal: &Balance) {
        let cf = self.db.cf_handle(CF_ADDR_BAL).unwrap();
        let key = addr.as_bytes();
        let val = {
            let mut vec = Vec::with_capacity(mem::size_of::<Balance>());
            vec.push_asset(&bal.gold);
            vec.push_asset(&bal.silver);
            vec
        };
        self.db.put_cf(cf, key, &val).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use rand::{thread_rng, Rng, distributions::Alphanumeric};
    use std::{env, fs, panic};
    use super::*;

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
            where F: FnOnce(Indexer) -> () + panic::UnwindSafe {
        let mut tmp_dir = env::temp_dir();
        let mut s = String::from("godcoin_test_");
        s.push_str(&thread_rng().sample_iter(&Alphanumeric).take(4).collect::<String>());
        tmp_dir.push(s);
        fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));

        let result = panic::catch_unwind(|| {
            let indexer = Indexer::new(&tmp_dir);
            func(indexer);
        });

        fs::remove_dir_all(&tmp_dir).expect("Failed to rm dir");
        assert!(result.is_ok());
    }
}
