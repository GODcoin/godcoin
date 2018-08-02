use rocksdb::{DB, ColumnFamilyDescriptor, Options};
use std::path::Path;
use serializer::*;

const CF_BLOCK_BYTE_POS: &str = "block_byte_pos";

pub struct Indexer {
    db: DB
}

impl Indexer {
    pub fn new(path: &Path) -> Indexer {
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);

        let col_families = vec![
            ColumnFamilyDescriptor::new(CF_BLOCK_BYTE_POS, Options::default())
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

        Some((u64::from(buf[0]) << 56)
                | (u64::from(buf[1]) << 48)
                | (u64::from(buf[2]) << 40)
                | (u64::from(buf[3]) << 32)
                | (u64::from(buf[4]) << 24)
                | (u64::from(buf[5]) << 16)
                | (u64::from(buf[6]) << 8)
                | u64::from(buf[7]))
    }

    pub fn set_block_byte_pos(&self, height: u64, pos: u64) {
        let mut key = Vec::with_capacity(8);
        key.push_u64(height);
        let mut val = Vec::with_capacity(8);
        val.push_u64(pos);

        let cf = self.db.cf_handle(CF_BLOCK_BYTE_POS).unwrap();
        self.db.put_cf(cf, &key, &val).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, panic};

    #[test]
    fn test_get_block_pos() {
        let mut tmp_dir = env::temp_dir();
        tmp_dir.push("godcoin_test");
        fs::create_dir(&tmp_dir).expect(&format!("Could not create temp dir {:?}", &tmp_dir));
        let indexer = Indexer::new(&tmp_dir);

        let result = panic::catch_unwind(|| {
            indexer.set_block_byte_pos(1, 327);
            assert!(indexer.get_block_byte_pos(0).is_none());
            assert_eq!(indexer.get_block_byte_pos(1).unwrap(), 327);
        });

        fs::remove_dir_all(&tmp_dir).expect("Failed to rm dir");

        assert!(result.is_ok());
    }
}
