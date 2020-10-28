use super::Entry;
use bytes::Bytes;
use std::collections::BTreeMap;

pub trait Storage: Send + 'static {
    /// Last irreversible index. Returning 0 signifies an empty or new log.
    fn stable_index(&self) -> u64;

    /// Commits the stable entries to persistent storage.
    fn commit_stable_entries(&mut self, entries: Vec<Entry>);

    fn retrieve_stable_entry(&self, index: u64) -> Option<Bytes>;
}

#[derive(Default, Debug)]
pub struct MemStorage {
    stable_entries: BTreeMap<u64, Entry>,
}

impl MemStorage {
    #[inline]
    #[cfg(test)]
    pub fn stable_entries(&self) -> &BTreeMap<u64, Entry> {
        &self.stable_entries
    }
}

impl Storage for MemStorage {
    /// Last irreversible index. Returning 0 signifies an empty or new log.
    fn stable_index(&self) -> u64 {
        self.stable_entries
            .iter()
            .rev()
            .next()
            .map_or(0, |(index, _)| *index)
    }

    /// Commits the stable entries to persistent storage.
    fn commit_stable_entries(&mut self, entries: Vec<Entry>) {
        for e in entries {
            self.stable_entries.insert(e.index, e);
        }
    }

    fn retrieve_stable_entry(&self, index: u64) -> Option<Bytes> {
        self.stable_entries.get(&index).map(|e| e.data.clone())
    }
}
