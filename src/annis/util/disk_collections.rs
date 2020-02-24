use crate::annis::errors::*;
use crate::annis::util::memory_estimation;
use malloc_size_of::{MallocSizeOf, MallocSizeOfOps};
use serde::{Deserialize, Serialize};
use sstable::{SSIterator, Table, TableBuilder, TableIterator};

use std::collections::BTreeMap;
use std::fs::File;
use std::iter::Peekable;
use std::ops::{Bound, RangeBounds};
use std::path::{Path, PathBuf};

mod serializer;

pub use serializer::KeySerializer;

const DEFAULT_MSG : &str = "Accessing the disk-database failed. This is a non-recoverable error since it means something serious is wrong with the disk or file system.";
const MAX_TRIES: usize = 5;

#[derive(Serialize, Deserialize)]
struct Entry<K, V>
where
    K: Ord,
{
    key: K,
    value: V,
}

pub enum EvictionStrategy {
    #[allow(dead_code)]
    MaximumItems(usize),
    MaximumBytes(usize),
}

impl Default for EvictionStrategy {
    fn default() -> Self {
        EvictionStrategy::MaximumBytes(16 * 1024 * 1024)
    }
}

pub struct DiskMap<K, V>
where
    K: 'static + KeySerializer + Send + Sync,
    for<'de> V: 'static + Serialize + Deserialize<'de> + Send + Sync,
{
    eviction_strategy: EvictionStrategy,
    c0: BTreeMap<Vec<u8>, Option<V>>,
    disk_tables: Vec<Table>,

    /// Marks if all items have been inserted in sorted order and if there has not been any delete operation yet.
    insertion_was_sorted: bool,
    last_inserted_key: Option<Vec<u8>>,

    serialization: bincode::Config,

    est_sum_memory: usize,

    phantom: std::marker::PhantomData<K>,
}

impl<K, V> DiskMap<K, V>
where
    K: 'static + Clone + KeySerializer + Send + Sync + MallocSizeOf,
    for<'de> V: 'static + Clone + Serialize + Deserialize<'de> + Send + Sync + MallocSizeOf,
{
    pub fn new(
        persisted_file: Option<&Path>,
        eviction_strategy: EvictionStrategy,
    ) -> Result<DiskMap<K, V>> {
        let serialization = bincode::config();

        let mut disk_tables = Vec::default();

        if let Some(persisted_file) = persisted_file {
            if persisted_file.is_file() {
                // Use existing file as read-only table which contains the whole map
                let table = Table::new_from_file(sstable::Options::default(), persisted_file)?;
                disk_tables.push(table);
            }
        }

        Ok(DiskMap {
            eviction_strategy,
            c0: BTreeMap::default(),
            disk_tables: Vec::default(),
            insertion_was_sorted: true,
            last_inserted_key: None,

            serialization: serialization,
            phantom: std::marker::PhantomData,
            est_sum_memory: 0,
        })
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        let binary_key = K::create_key(&key);

        let mut mem_ops =
            MallocSizeOfOps::new(memory_estimation::platform::usable_size, None, None);
        let binary_key_size = binary_key.size_of(&mut mem_ops);

        // Add memory size for inserted element
        self.est_sum_memory +=
            std::mem::size_of::<(Vec<u8>, V)>() + binary_key_size + value.size_of(&mut mem_ops);

        // Check if insertion is still sorted
        if self.insertion_was_sorted {
            if let Some(last_key) = &self.last_inserted_key {
                self.insertion_was_sorted = last_key < &binary_key;
            }
            self.last_inserted_key = Some(binary_key.clone());
        }

        let existing_c0_entry = self.c0.insert(binary_key, Some(value));
        if let Some(existing) = &existing_c0_entry {
            // Subtract the memory size for the item that was removed
            self.est_sum_memory -= std::mem::size_of::<(Vec<u8>, V)>()
                + binary_key_size
                + existing.size_of(&mut mem_ops);
        }

        self.check_eviction_necessary(true)?;

        Ok(())
    }

    fn check_eviction_necessary(&mut self, write_deleted: bool) -> Result<()> {
        match self.eviction_strategy {
            EvictionStrategy::MaximumItems(n) => {
                if self.c0.len() > n {
                    self.evict_c0(write_deleted, None)?;
                }
            }
            EvictionStrategy::MaximumBytes(b) => {
                if self.est_sum_memory > b {
                    self.evict_c0(write_deleted, None)?;
                }
            }
        }
        Ok(())
    }

    fn evict_c0(&mut self, write_deleted: bool, output_file: Option<&PathBuf>) -> Result<()> {
        let out_file = if let Some(output_file) = output_file {
            debug!("Evicting DiskMap C0 to {:?}", output_file.as_path());
            if let Some(parent) = output_file.parent() {
                std::fs::create_dir_all(parent)?
            }
            std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .create(true)
                .open(output_file)?
        } else {
            debug!("Evicting DiskMap C0 to temporary file");
            tempfile::tempfile()?
        };

        {
            let mut builder = TableBuilder::new(sstable::Options::default(), &out_file);

            for (key, value) in self.c0.iter() {
                let key = key.create_key();
                if write_deleted || value.is_some() {
                    builder.add(&key, &self.serialization.serialize(value)?)?;
                }
            }
            builder.finish()?;
        }

        self.est_sum_memory = 0;
        let size = out_file.metadata()?.len();
        let table = Table::new(
            sstable::Options::default(),
            Box::new(out_file),
            size as usize,
        )?;
        self.disk_tables.push(table);

        self.c0.clear();

        debug!("Finished evicting DiskMap C0 ");
        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove(&mut self, key: &K) -> Result<Option<V>> {
        let key = K::create_key(key);

        let existing = self.get_raw(&key)?;
        if existing.is_some() {
            let mut mem_ops =
                MallocSizeOfOps::new(memory_estimation::platform::usable_size, None, None);

            self.est_sum_memory -= existing.size_of(&mut mem_ops);

            // Add tombstone entry
            let empty_value = None;
            self.est_sum_memory += empty_value.size_of(&mut mem_ops);
            self.c0.insert(key, empty_value);

            self.insertion_was_sorted = false;

            self.check_eviction_necessary(true)?;
        }
        Ok(existing)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.c0.clear();
        self.disk_tables.clear();
        self.est_sum_memory = 0;
        self.insertion_was_sorted = true;
        self.last_inserted_key = None;
    }

    pub fn try_get(&self, key: &K) -> Result<Option<V>> {
        let key = K::create_key(key);
        self.get_raw(&key)
    }

    /// Returns an optional value for the given key.
    ///
    /// # Panics
    ///
    /// The will try to query the disk-based map several times
    /// If a maximum number of tries is reached and all attempts failed, this will panic.
    #[allow(dead_code)]
    pub fn get(&self, key: &K) -> Option<V> {
        let mut last_err = None;
        for _ in 0..MAX_TRIES {
            match self.try_get(key) {
                Ok(result) => return result,
                Err(e) => last_err = Some(e),
            }
            // If this is an intermediate error, wait some time before trying again
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        panic!("{}\nCause:\n{:?}", DEFAULT_MSG, last_err.unwrap())
    }

    fn get_raw(&self, key: &Vec<u8>) -> Result<Option<V>> {
        // Check C0 first
        if let Some(value) = self.c0.get(key) {
            if value.is_some() {
                return Ok(value.clone());
            } else {
                // Value was explicitly deleted, do not query the disk tables
                return Ok(None);
            }
        }
        // Iterate over all disk-tables to find the entry
        for table in self.disk_tables.iter().rev() {
            if let Some(value) = table.get(key)? {
                let value: Option<V> = self.serialization.deserialize(&value)?;
                if value.is_some() {
                    return Ok(value);
                } else {
                    // Value was explicitly deleted, do not query the rest of the disk tables
                    return Ok(None);
                }
            }
        }

        Ok(None)
    }

    pub fn try_contains_key(&self, key: &K) -> Result<bool> {
        self.try_get(key).map(|item| item.is_some())
    }

    /// Returns if the given key is contained.
    ///
    /// # Panics
    ///
    /// The will try to query the disk-based map several times
    /// If a maximum number of tries is reached and all attempts failed, this will panic.
    #[allow(dead_code)]
    pub fn contains_key(&self, key: &K) -> bool {
        let mut last_err = None;
        for _ in 0..MAX_TRIES {
            match self.try_contains_key(key) {
                Ok(result) => return result,
                Err(e) => last_err = Some(e),
            }
            // If this is an intermediate error, wait some time before trying again
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        panic!("{}\nCause:\n{:?}", DEFAULT_MSG, last_err.unwrap())
    }

    pub fn try_is_empty(&self) -> Result<bool> {
        if self.c0.is_empty() && self.disk_tables.is_empty() {
            return Ok(true);
        }
        let mut it = self.try_iter()?;
        Ok(it.next().is_none())
    }

    /// Returns if the map is empty
    ///
    /// # Panics
    ///
    /// The will try to query the disk-based map several times
    /// If a maximum number of tries is reached and all attempts failed, this will panic.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        let mut last_err = None;
        for _ in 0..MAX_TRIES {
            match self.try_is_empty() {
                Ok(result) => return result,
                Err(e) => last_err = Some(e),
            }
            // If this is an intermediate error, wait some time before trying again
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        panic!("{}\nCause:\n{:?}", DEFAULT_MSG, last_err.unwrap())
    }

    pub fn try_iter<'a>(&'a self) -> Result<Box<dyn Iterator<Item = (K, V)> + 'a>> {
        if self.insertion_was_sorted {
            // Use a less complicated and faster iterator over all items
            let mut remaining_table_iterators = Vec::with_capacity(self.disk_tables.len());
            // The disk tables are sorted by oldest first. Reverse the order to have the oldest ones last, so that
            // calling "pop()" will return older disk tables first.
            for t in self.disk_tables.iter().rev() {
                let it = t.iter();
                remaining_table_iterators.push(it);
            }
            let current_table_iterator = remaining_table_iterators.pop();
            let it = SortedLogTableIterator {
                c0_iterator: self.c0.iter(),
                current_table_iterator,
                remaining_table_iterators,
                serialization: self.serialization.clone(),
                phantom: std::marker::PhantomData,
            };
            Ok(Box::new(it))
        } else {
            // Default to an iterator that can handle non-globally sorted tables
            let it = self.try_range(..)?;
            Ok(Box::new(it))
        }
    }

    /// Returns an iterator over the all entries.
    ///
    /// # Panics
    ///
    /// The will try to query the disk-based map several times
    /// If a maximum number of tries is reached and all attempts failed, this will panic.
    #[allow(dead_code)]
    pub fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (K, V)> + 'a> {
        let mut last_err = None;
        for _ in 0..MAX_TRIES {
            match self.try_iter() {
                Ok(result) => return result,
                Err(e) => last_err = Some(e),
            }
            // If this is an intermediate error, wait some time before trying again
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        panic!("{}\nCause:\n{:?}", DEFAULT_MSG, last_err.unwrap())
    }

    pub fn try_range<R>(&self, range: R) -> Result<Range<K, V>>
    where
        R: RangeBounds<K> + Clone,
    {
        let mut table_iterators: Vec<TableIterator> = self
            .disk_tables
            .iter()
            .rev()
            .map(|table| table.iter())
            .collect();
        let mut exhausted: Vec<bool> = std::iter::repeat(false)
            .take(table_iterators.len())
            .collect();

        let mapped_start_bound = match range.start_bound() {
            Bound::Included(end) => Bound::Included(K::create_key(end)),
            Bound::Excluded(end) => Bound::Excluded(K::create_key(end)),
            Bound::Unbounded => Bound::Unbounded,
        };

        let mapped_end_bound = match range.end_bound() {
            Bound::Included(end) => Bound::Included(K::create_key(end)),
            Bound::Excluded(end) => Bound::Excluded(K::create_key(end)),
            Bound::Unbounded => Bound::Unbounded,
        };

        match &mapped_start_bound {
            Bound::Included(start) => {
                let mut key = Vec::default();
                let mut value = Vec::default();

                for i in 0..table_iterators.len() {
                    let exhausted = &mut exhausted[i];
                    let ti = &mut table_iterators[i];
                    ti.seek(&start);

                    if ti.valid() && ti.current(&mut key, &mut value) {
                        // Check if the seeked element is actually part of the range
                        let start_included = match &mapped_start_bound {
                            Bound::Included(start) => &key >= start,
                            Bound::Excluded(start) => &key > start,
                            Bound::Unbounded => true,
                        };
                        let end_included = match &mapped_end_bound {
                            Bound::Included(end) => &key <= end,
                            Bound::Excluded(end) => &key < end,
                            Bound::Unbounded => true,
                        };
                        if !start_included || !end_included {
                            *exhausted = true;
                        }
                    } else {
                        // Seeked behind last element
                        *exhausted = true;
                    }
                }
            }
            Bound::Excluded(start_bound) => {
                let mut key: Vec<u8> = Vec::default();
                let mut value = Vec::default();

                for i in 0..table_iterators.len() {
                    let exhausted = &mut exhausted[i];
                    let ti = &mut table_iterators[i];

                    ti.seek(&start_bound);
                    if ti.valid() && ti.current(&mut key, &mut value) {
                        if &key == start_bound {
                            // We need to exclude the first match
                            ti.advance();
                        }
                    }

                    // Check key after advance
                    if ti.valid() && ti.current(&mut key, &mut value) {
                        // Check if the seeked element is actually part of the range
                        let start_included = match &mapped_start_bound {
                            Bound::Included(start) => &key >= start,
                            Bound::Excluded(start) => &key > start,
                            Bound::Unbounded => true,
                        };
                        let end_included = match &mapped_end_bound {
                            Bound::Included(end) => &key <= end,
                            Bound::Excluded(end) => &key < end,
                            Bound::Unbounded => true,
                        };
                        if !start_included || !end_included {
                            *exhausted = true;
                        }
                    } else {
                        // Seeked behind last element
                        *exhausted = true;
                    }
                }
            }
            Bound::Unbounded => {
                for i in 0..table_iterators.len() {
                    let exhausted = &mut exhausted[i];
                    let ti = &mut table_iterators[i];

                    ti.seek_to_first();

                    if !ti.valid() {
                        *exhausted = true;
                    }
                }
            }
        };

        Ok(Range {
            c0_range: self
                .c0
                .range((mapped_start_bound.clone(), mapped_end_bound.clone()))
                .peekable(),
            range_start: mapped_start_bound,
            range_end: mapped_end_bound,
            exhausted,
            table_iterators,
            serialization: self.serialization.clone(),
            phantom: std::marker::PhantomData,
        })
    }

    /// Returns an iterator over a range of entries.
    ///
    /// # Panics
    ///
    /// The will try to query the disk-based map several times
    /// If a maximum number of tries is reached and all attempts failed, this will panic.
    #[allow(dead_code)]
    pub fn range<R>(&self, range: R) -> Range<K, V>
    where
        R: RangeBounds<K> + Clone,
    {
        let mut last_err = None;
        for _ in 0..MAX_TRIES {
            match self.try_range(range.clone()) {
                Ok(result) => return result,
                Err(e) => last_err = Some(e),
            }
            // If this is an intermediate error, wait some time before trying again
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        panic!("{}\nCause:\n{:?}", DEFAULT_MSG, last_err.unwrap())
    }

    /// Merges two disk tables.
    /// Newer entries overwrite older ones from the base table.
    ///
    /// - `write_deleted` - If `true`, tombstones for deleted entries are preserved and written to disk
    fn merge_disk_tables(
        &self,
        older: &Table,
        newer: &Table,
        file: &File,
        write_deleted: bool,
    ) -> Result<()> {
        let mut builder = TableBuilder::new(sstable::Options::default(), file);

        let mut it_older = older.iter();
        let mut it_newer = newer.iter();

        let mut k_newer = Vec::default();
        let mut v_newer = Vec::default();

        let mut k_older = Vec::default();
        let mut v_older = Vec::default();

        it_newer.seek_to_first();
        it_older.seek_to_first();

        while it_older.current(&mut k_older, &mut v_older)
            && it_newer.current(&mut k_newer, &mut v_newer)
        {
            if k_older < k_newer {
                // Add the value from the older table
                if write_deleted {
                    builder.add(&k_older, &v_older)?;
                } else {
                    let parsed: Option<V> = self.serialization.deserialize(&v_older)?;
                    if parsed.is_some() {
                        builder.add(&k_older, &v_older)?;
                    }
                }
                it_older.advance();
            } else if k_older > k_newer {
                // Add the value from the newer table
                if write_deleted {
                    builder.add(&k_newer, &v_newer)?;
                } else {
                    let parsed: Option<V> = self.serialization.deserialize(&v_newer)?;
                    if parsed.is_some() {
                        builder.add(&k_newer, &v_newer)?;
                    }
                }
                it_newer.advance();
            } else {
                // Use the newer values for the same keys
                if write_deleted {
                    builder.add(&k_newer, &v_newer)?;
                } else {
                    let parsed: Option<V> = self.serialization.deserialize(&v_newer)?;
                    if parsed.is_some() {
                        builder.add(&k_newer, &v_newer)?;
                    }
                }
                it_older.advance();
                it_newer.advance();
            }
        }

        // The above loop will stop when one or both of the iterators are exhausted.
        // We need to insert the remaining items of the other table as well
        if it_newer.valid() {
            while it_newer.current(&mut k_newer, &mut v_newer) {
                if write_deleted {
                    builder.add(&k_newer, &v_newer)?;
                } else {
                    let parsed: Option<V> = self.serialization.deserialize(&v_newer)?;
                    if parsed.is_some() {
                        builder.add(&k_newer, &v_newer)?;
                    }
                }
                it_newer.advance();
            }
        } else if it_older.valid() {
            while it_older.current(&mut k_older, &mut v_older) {
                if write_deleted {
                    builder.add(&k_older, &v_older)?;
                } else {
                    let parsed: Option<V> = self.serialization.deserialize(&v_older)?;
                    if parsed.is_some() {
                        builder.add(&k_older, &v_older)?;
                    }
                }
                it_older.advance();
            }
        }

        builder.finish()?;

        Ok(())
    }

    /// Compact the existing disk tables and the in-memory table to a single temporary disk table.
    pub fn compact(&mut self) -> Result<()> {
        self.est_sum_memory = 0;

        if self.c0.is_empty() && self.disk_tables.is_empty() {
            // The table is completly empty.
            return Ok(());
        }
        if !self.c0.is_empty() {
            // Make sure all entries of C0 are written to disk.
            // Ommit all deleted entries if this becomes the only, and therefore complete, disk table
            self.evict_c0(!self.disk_tables.is_empty(), None)?;
        }

        // More recent entries are always appended to the end.
        // To make it easier to pop entries we are reversing the vector once, so calling "pop" will always return
        // the oldest entry.
        // We don't need to reverse again after the compaction, because there will be only at most one entry left.
        self.disk_tables.reverse();

        debug!(
            "Merging {} disk-based tables in DiskMap",
            self.disk_tables.len()
        );

        // Start from the end of disk tables (now containing the older entries) and merge them pairwise into temporary tables
        let mut base_optional = self.disk_tables.pop();
        let mut newer_optional = self.disk_tables.pop();
        while let (Some(base), Some(newer)) = (&base_optional, &newer_optional) {
            let is_last_table = self.disk_tables.is_empty();
            // When merging the last two tables, prune the deleted entries
            let write_deleted = !is_last_table;

            // After evicting C0 and merging all tables, a single disk-table will be created.
            // Use a temporary file to save the table.
            let table_file = tempfile::tempfile()?;
            self.merge_disk_tables(base, newer, &table_file, write_deleted)?;
            // Re-Open created table as "older" table
            let size = table_file.metadata()?.len() as usize;
            let table = Table::new(sstable::Options::default(), Box::from(table_file), size)?;
            base_optional = Some(table);
            // Prepare merging with the next younger table from the log
            newer_optional = self.disk_tables.pop();
        }

        if let Some(table) = base_optional {
            self.disk_tables = vec![table];
        }

        debug!("Finished merging disk-based tables in DiskMap");

        Ok(())
    }

    pub fn write_to(&self, location: &Path) -> Result<()> {
        // Open file as writable
        let out_file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(&location)?;
        let mut builder = TableBuilder::new(sstable::Options::default(), out_file);
        for (key, value) in self.c0.iter() {
            let key = key.create_key();
            if value.is_some() {
                builder.add(&key, &self.serialization.serialize(value)?)?;
            }
        }
        builder.finish()?;

        Ok(())
    }
}

impl<K, V> Default for DiskMap<K, V>
where
    K: 'static + Clone + KeySerializer + Send + Sync + MallocSizeOf,
    for<'de> V: 'static + Clone + Serialize + Deserialize<'de> + Send + Sync + MallocSizeOf,
{
    fn default() -> Self {
        DiskMap::new(None, EvictionStrategy::default())
            .expect("Temporary disk map creation should not fail.")
    }
}

pub struct Range<'a, K, V> {
    range_start: Bound<Vec<u8>>,
    range_end: Bound<Vec<u8>>,
    c0_range: Peekable<std::collections::btree_map::Range<'a, Vec<u8>, Option<V>>>,
    table_iterators: Vec<TableIterator>,
    exhausted: Vec<bool>,
    serialization: bincode::Config,
    phantom: std::marker::PhantomData<(K, V)>,
}

impl<'a, K, V> Range<'a, K, V>
where
    for<'de> K: 'static + Clone + KeySerializer + Send,
    for<'de> V: 'static + Clone + Serialize + Deserialize<'de> + Send,
{
    fn range_contains(&self, item: &Vec<u8>) -> bool {
        (match &self.range_start {
            Bound::Included(ref start) => start <= item,
            Bound::Excluded(ref start) => start < item,
            Bound::Unbounded => true,
        }) && (match &self.range_end {
            Bound::Included(ref end) => item <= end,
            Bound::Excluded(ref end) => item < end,
            Bound::Unbounded => true,
        })
    }

    fn advance_all(&mut self, after_key: &Vec<u8>) {
        // Skip all smaller or equal keys in C0
        while let Some(c0_item) = self.c0_range.peek() {
            if c0_item.0 <= after_key {
                self.c0_range.next();
            } else {
                break;
            }
        }

        // Skip all smaller or equal keys in all disk tables
        for i in 0..self.table_iterators.len() {
            if self.exhausted[i] == false && self.table_iterators[i].valid() {
                let mut key = Vec::default();
                let mut value = Vec::default();
                if self.table_iterators[i].current(&mut key, &mut value) {
                    if !self.range_contains(&key) {
                        self.exhausted[i] = true;
                        break;
                    }
                    if &key <= after_key {
                        self.table_iterators[i].advance();
                    }
                }
            }
        }
    }
}

impl<'a, K, V> Iterator for Range<'a, K, V>
where
    for<'de> K: 'static + Clone + KeySerializer + Send,
    for<'de> V: 'static + Clone + Serialize + Deserialize<'de> + Send,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<(K, V)> {
        loop {
            // Find the smallest key in all tables.
            let mut smallest_key: Option<(Vec<u8>, Option<V>)> = None;

            // Try C0 first
            if let Some(c0_item) = self.c0_range.peek() {
                let key: &Vec<u8> = c0_item.0;
                let value: &Option<V> = c0_item.1;
                smallest_key = Some((key.clone(), value.clone()));
            }

            // Iterate over all disk tables
            for i in 0..self.table_iterators.len() {
                let table_it = &mut self.table_iterators[i];

                if self.exhausted[i] == false && table_it.valid() {
                    let mut key = Vec::default();
                    let mut value = Vec::default();
                    if table_it.current(&mut key, &mut value) {
                        if self.range_contains(&key) {
                            let value: Option<V> = self
                                .serialization
                                .deserialize(&value)
                                .expect("Could not decode previously written data from disk.");
                            smallest_key = Some((key, value));
                        } else {
                            self.exhausted[i] = true;
                        }
                    }
                }
            }

            if let Some(smallest_key) = smallest_key {
                // Set all iterators to the next element
                self.advance_all(&smallest_key.0);
                // Return any non-deleted entry
                if let Some(value) = smallest_key.1 {
                    let key = K::parse_key(&smallest_key.0);
                    return Some((key, value));
                }
            } else {
                // All iterators are exhausted
                return None;
            }
        }
    }
}

/// Implements an optimized iterator over C0 and all disk tables.
/// This iterator assumes the table entries have been inserted in sorted
/// order and no delete has occurred.
struct SortedLogTableIterator<'a, K, V> {
    current_table_iterator: Option<TableIterator>,
    remaining_table_iterators: Vec<TableIterator>,
    c0_iterator: std::collections::btree_map::Iter<'a, Vec<u8>, Option<V>>,
    serialization: bincode::Config,
    phantom: std::marker::PhantomData<K>,
}

impl<'a, K, V> Iterator for SortedLogTableIterator<'a, K, V>
where
    for<'de> K: 'static + Clone + KeySerializer + Send,
    for<'de> V: 'static + Clone + Serialize + Deserialize<'de> + Send,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<(K, V)> {
        while let Some(t) = &mut self.current_table_iterator {
            if let Some((key, value)) = t.next() {
                let key = K::parse_key(&key);
                let value: Option<V> = self
                    .serialization
                    .deserialize(&value)
                    .expect("Could not decode previously written data from disk.");
                if let Some(value) = value {
                    return Some((key, value.clone()));
                } else {
                    panic!("Optimized log table iterator should have been called only if no entry was ever deleted");
                }
            } else {
                self.current_table_iterator = self.remaining_table_iterators.pop();
            }
        }
        // Check C0 (which contains the newest entries)
        if let Some((key, value)) = self.c0_iterator.next() {
            let key = K::parse_key(&key);
            if let Some(value) = value {
                return Some((key, value.clone()));
            } else {
                panic!("Optimized log table iterator should have been called only if no entry was ever deleted");
            }
        } else {
        }

        None
    }
}

#[cfg(test)]
mod tests;
