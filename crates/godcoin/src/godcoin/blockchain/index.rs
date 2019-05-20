use rocksdb::{ColumnFamilyDescriptor, DBRecoveryMode, Options, DB};
use std::{collections::HashMap, io::Cursor, mem, path::Path, sync::Arc};

use crate::{
    asset::Asset,
    crypto::ScriptHash,
    serializer::*,
    tx::{OwnerTx, TxVariant},
};

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

    pub fn get_chain_height(&self) -> u64 {
        match self.db.get(KEY_CHAIN_HEIGHT).unwrap() {
            Some(val) => u64_from_buf!(val),
            None => 0,
        }
    }

    pub fn get_owner(&self) -> Option<OwnerTx> {
        let tx_buf = self.db.get(KEY_NET_OWNER).unwrap()?;
        let cur = &mut Cursor::<&[u8]>::new(&tx_buf);
        let tx = TxVariant::deserialize(cur).unwrap();
        match tx {
            TxVariant::OwnerTx(owner) => Some(owner),
            _ => panic!("expected owner transaction"),
        }
    }

    pub fn get_balance(&self, hash: &ScriptHash) -> Option<Asset> {
        let cf = self.db.cf_handle(CF_ADDR_BAL).unwrap();
        let bal_buf = self.db.get_cf(cf, hash.as_ref()).unwrap()?;
        let cur = &mut Cursor::<&[u8]>::new(&bal_buf);
        let bal = cur.take_asset().unwrap();
        Some(bal)
    }

    pub fn get_token_supply(&self) -> Asset {
        let supply_buf = self.db.get(KEY_TOKEN_SUPPLY).unwrap();
        match supply_buf {
            Some(supply_buf) => {
                let cur = &mut Cursor::<&[u8]>::new(&supply_buf);
                cur.take_asset().unwrap()
            }
            None => Asset::default(),
        }
    }
}

pub struct WriteBatch {
    indexer: Arc<Indexer>,
    block_byte_pos: HashMap<u64, u64>,
    chain_height: Option<u64>,
    owner: Option<OwnerTx>,
    balances: HashMap<ScriptHash, Asset>,
    token_supply: Option<Asset>,
}

impl WriteBatch {
    pub fn new(indexer: Arc<Indexer>) -> Self {
        WriteBatch {
            indexer,
            block_byte_pos: HashMap::with_capacity(1),
            chain_height: None,
            owner: None,
            balances: HashMap::with_capacity(64),
            token_supply: None,
        }
    }

    pub fn commit(self) {
        let mut batch = rocksdb::WriteBatch::default();

        {
            let cf = self.indexer.db.cf_handle(CF_BLOCK_BYTE_POS).unwrap();
            for (height, pos) in self.block_byte_pos {
                let height = height.to_be_bytes();
                let pos = pos.to_be_bytes();
                batch.put_cf(cf, &height, &pos).unwrap();
            }
        }

        if let Some(height) = self.chain_height {
            batch.put(KEY_CHAIN_HEIGHT, height.to_be_bytes()).unwrap();
        }

        if let Some(owner) = self.owner {
            let val = {
                let mut vec = Vec::with_capacity(4096);
                TxVariant::OwnerTx(owner).serialize(&mut vec);
                vec
            };
            batch.put(KEY_NET_OWNER, &val).unwrap();
        }

        if let Some(token_supply) = self.token_supply {
            let val = {
                let mut buf = Vec::with_capacity(mem::size_of::<Asset>());
                buf.push_asset(token_supply);
                buf
            };
            batch.put(KEY_TOKEN_SUPPLY, &val).unwrap();
        }

        {
            let cf = self.indexer.db.cf_handle(CF_ADDR_BAL).unwrap();
            let mut buf = Vec::with_capacity(mem::size_of::<Asset>());
            for (addr, bal) in self.balances {
                buf.push_asset(bal);
                batch.put_cf(cf, addr.as_ref(), &buf).unwrap();
                buf.clear();
            }
        }

        self.indexer.db.write(batch).unwrap();
    }

    pub fn set_block_byte_pos(&mut self, height: u64, pos: u64) {
        self.block_byte_pos.insert(height, pos);
    }

    pub fn set_chain_height(&mut self, height: u64) {
        self.chain_height = Some(height);
    }

    pub fn set_owner(&mut self, owner: OwnerTx) {
        self.owner = Some(owner);
    }

    pub fn add_token_supply(&mut self, amount: Asset) {
        match self.token_supply.as_mut() {
            Some(token_supply) => {
                *token_supply = token_supply.add(amount).unwrap();
            }
            None => {
                let amt = self.indexer.get_token_supply().add(amount).unwrap();
                self.token_supply = Some(amt);
            }
        }
    }

    pub fn add_bal(&mut self, addr: &ScriptHash, amount: Asset) {
        match self.balances.get_mut(addr) {
            Some(bal) => {
                *bal = bal.add(amount).unwrap();
            }
            None => {
                let bal = self
                    .indexer
                    .get_balance(addr)
                    .unwrap_or_else(Default::default)
                    .add(amount)
                    .unwrap();
                self.balances.insert(addr.clone(), bal);
            }
        }
    }

    pub fn sub_bal(&mut self, addr: &ScriptHash, amount: Asset) {
        match self.balances.get_mut(addr) {
            Some(bal) => {
                *bal = bal.sub(amount).unwrap();
            }
            None => {
                let bal = self
                    .indexer
                    .get_balance(addr)
                    .unwrap_or_else(Default::default)
                    .sub(amount)
                    .unwrap();
                self.balances.insert(addr.clone(), bal);
            }
        }
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
            let mut batch = WriteBatch::new(Arc::clone(&indexer));
            batch.set_block_byte_pos(1, 327);
            batch.commit();
            assert!(indexer.get_block_byte_pos(0).is_none());
            assert_eq!(indexer.get_block_byte_pos(1).unwrap(), 327);
        });
    }

    #[test]
    fn test_get_chain_height() {
        run_test(|indexer| {
            assert_eq!(indexer.get_chain_height(), 0);
            let mut batch = WriteBatch::new(Arc::clone(&indexer));
            batch.set_chain_height(42);
            batch.commit();
            assert_eq!(indexer.get_chain_height(), 42);
        });
    }

    fn run_test<F>(func: F)
    where
        F: FnOnce(Arc<Indexer>) -> () + panic::UnwindSafe,
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
            func(Arc::new(indexer));
        });

        fs::remove_dir_all(&tmp_dir).expect("Failed to rm dir");
        assert!(result.is_ok());
    }
}
