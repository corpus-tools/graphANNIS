use crate::annis::db::annostorage::AnnotationStorage;
use crate::annis::db::Match;
use crate::annis::db::ValueSearch;
use crate::annis::errors::Result;
use crate::annis::types::AnnoKey;
use crate::annis::types::AnnoKeyID;
use crate::annis::types::Annotation;
use crate::malloc_size_of::MallocSizeOf;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use sanakirja::value::UnsafeValue;
use sanakirja::{Commit, Db, Env, MutTxn, Representable, Transaction};
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::Path;

const BY_CONTAINER_ID: usize = 0;
const BY_ANNO_ID: usize = 0;

type AnnotationsDb = Db<(UnsafeValue, UnsafeValue), UnsafeValue>;
type ByContainerDb<T> = Db<T, AnnotationsDb>;

type ValuesDb<T> = Db<UnsafeValue, T>;
type ByAnnoDb<T> = Db<(UnsafeValue, UnsafeValue), ValuesDb<T>>;

#[derive(MallocSizeOf)]
pub struct AnnoStorageImpl<T: Ord + Hash + MallocSizeOf + Default + Representable> {
    phantom: PhantomData<T>,

    #[ignore_malloc_size_of = "state of environment is negligible compared to the actual maps (which are non disk)"]
    env: Env,
}

impl<T: Ord + Hash + MallocSizeOf + Default + Representable> AnnoStorageImpl<T> {
    pub fn load_from_file(path: &str) -> Result<AnnoStorageImpl<T>> {
        let path = Path::new(path);
        // Use 100 MB (SI standard) as default size
        let env = Env::new(path, 100_000_000)?;
        let mut txn = env.mut_txn_begin()?;

        Self::create_non_existing_roots(&mut txn)?;

        txn.commit()?;

        Ok(AnnoStorageImpl {
            env,
            phantom: PhantomData::default(),
        })
    }

    fn create_non_existing_roots<A>(txn: &mut MutTxn<A>) -> Result<()> {
        // Map from the item to all its annotations (as a map with the name/namespace as key)
        let by_container: Option<ByContainerDb<T>> = txn.root(BY_CONTAINER_ID);
        if by_container.is_none() {
            let root: ByContainerDb<T> = txn.create_db()?;
            txn.set_root(BY_CONTAINER_ID, root);
        }

        // A map from an annotation key to a map of all its values to the items having this value for the annotation key
        let by_anno: Option<ByAnnoDb<T>> = txn.root(BY_ANNO_ID);
        if by_anno.is_none() {
            let root: ByAnnoDb<T> = txn.create_db()?;
            txn.set_root(BY_ANNO_ID, root);
        }

        Ok(())
    }

    fn insert_internal(&mut self, item: T, anno: Annotation) -> Result<()> {
        let mut rng = SmallRng::from_rng(rand::thread_rng())?;
        let mut txn = self.env.mut_txn_begin()?;

        let by_container: Option<ByContainerDb<T>> = txn.root(BY_CONTAINER_ID);
        let by_anno: Option<ByAnnoDb<T>> = txn.root(BY_ANNO_ID);

        if let (Some(mut by_container), Some(mut by_anno)) = (by_container, by_anno) {
            let name = UnsafeValue::from_slice(anno.key.name.as_bytes());
            let namespace = UnsafeValue::from_slice(anno.key.ns.as_bytes());
            let val = UnsafeValue::from_slice(anno.val.as_bytes());

            // try to get an existing kv for all annotations of this item or create a new one
            let mut annotations: AnnotationsDb =
                if let Some(existing) = txn.get(&by_container, item, None) {
                    existing
                } else {
                    let created_annotations_db = txn.create_db()?;
                    txn.put(&mut rng, &mut by_container, item, created_annotations_db)?;
                    created_annotations_db
                };
            // add the annotation value to the corresponding name/namespace pair
            txn.put(&mut rng, &mut annotations, (name, namespace), val)?;

            // add item to the by_anno map and also create the values kv if not yet existing
            let mut values_for_anno: ValuesDb<T> =
                if let Some(existing) = txn.get(&by_anno, (name, namespace), None) {
                    existing
                } else {
                    let created_values_db = txn.create_db()?;
                    txn.put(&mut rng, &mut by_anno, (name, namespace), created_values_db)?;
                    created_values_db
                };
            txn.put(&mut rng, &mut values_for_anno, val, item)?;
        }

        txn.commit()?;
        Ok(())
    }

    fn clear_internal(&mut self) -> Result<()> {
        let mut rng = SmallRng::from_rng(rand::thread_rng())?;
        let mut txn = self.env.mut_txn_begin()?;

        // drop the existing DBs
        let by_container: Option<ByContainerDb<T>> = txn.root(BY_CONTAINER_ID);
        if let Some(by_container) = by_container {
            txn.drop(&mut rng, &by_container)?;
        }

        let by_anno: Option<ByAnnoDb<T>> = txn.root(BY_ANNO_ID);
        if let Some(by_anno) = by_anno {
            txn.drop(&mut rng, &by_anno)?;
        }

        // re-create the dropped DBs
        Self::create_non_existing_roots(&mut txn)?;

        txn.commit()?;
        Ok(())
    }
}

impl<'de, T> AnnotationStorage<T> for AnnoStorageImpl<T>
where
    T: Ord + Hash + MallocSizeOf + Default + Clone + Representable + Send + Sync,
    (T, AnnoKeyID): Into<Match>,
{
    fn insert(&mut self, item: T, anno: Annotation) {
        if let Err(e) = self.insert_internal(item, anno) {
            error!("Could not insert value into node annotation storage: {}", e);
        }
    }

    fn get_all_keys_for_item(&self, _item: &T) -> Vec<AnnoKey> {
        unimplemented!()
    }

    fn remove_annotation_for_item(&mut self, _item: &T, _key: &AnnoKey) -> Option<String> {
        unimplemented!()
    }

    fn clear(&mut self) {
        if let Err(e) = self.clear_internal() {
            error!("Could not clear node annotation storage: {}", e);
        }
    }

    fn get_qnames(&self, _name: &str) -> Vec<AnnoKey> {
        unimplemented!()
    }

    fn get_key_id(&self, _key: &AnnoKey) -> Option<AnnoKeyID> {
        unimplemented!()
    }

    fn get_key_value(&self, _key_id: AnnoKeyID) -> Option<AnnoKey> {
        unimplemented!()
    }

    fn get_annotations_for_item(&self, _item: &T) -> Vec<Annotation> {
        unimplemented!()
    }

    fn number_of_annotations(&self) -> usize {
        unimplemented!()
    }

    fn get_value_for_item(&self, _item: &T, _key: &AnnoKey) -> Option<&str> {
        unimplemented!()
    }

    fn get_value_for_item_by_id(&self, _item: &T, _key_id: AnnoKeyID) -> Option<&str> {
        unimplemented!()
    }

    fn number_of_annotations_by_name(&self, _ns: Option<String>, _name: String) -> usize {
        unimplemented!()
    }

    fn exact_anno_search<'a>(
        &'a self,
        _namespace: Option<String>,
        _name: String,
        _value: ValueSearch<String>,
    ) -> Box<Iterator<Item = Match> + 'a> {
        unimplemented!()
    }

    fn regex_anno_search<'a>(
        &'a self,
        _namespace: Option<String>,
        _name: String,
        _pattern: &str,
        _negated: bool,
    ) -> Box<Iterator<Item = Match> + 'a> {
        unimplemented!()
    }

    fn find_annotations_for_item(
        &self,
        _item: &T,
        _ns: Option<String>,
        _name: Option<String>,
    ) -> Vec<AnnoKeyID> {
        unimplemented!()
    }

    fn guess_max_count(
        &self,
        _ns: Option<String>,
        _name: String,
        _lower_val: &str,
        _upper_val: &str,
    ) -> usize {
        unimplemented!()
    }

    fn guess_max_count_regex(&self, _ns: Option<String>, _name: String, _pattern: &str) -> usize {
        unimplemented!()
    }

    fn guess_most_frequent_value(&self, _ns: Option<String>, _name: String) -> Option<String> {
        unimplemented!()
    }

    fn get_all_values(&self, _key: &AnnoKey, _most_frequent_first: bool) -> Vec<&str> {
        unimplemented!()
    }

    fn annotation_keys(&self) -> Vec<AnnoKey> {
        unimplemented!()
    }

    fn get_largest_item(&self) -> Option<T> {
        unimplemented!()
    }

    fn calculate_statistics(&mut self) {
        unimplemented!()
    }
}
