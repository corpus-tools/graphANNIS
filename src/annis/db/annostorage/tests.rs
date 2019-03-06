use super::*;
use crate::annis::types::NodeID;

#[test]
fn insert_same_anno() {
    let test_anno = Annotation {
        key: AnnoKey {
            name: "anno1".to_owned(),
            ns: "annis".to_owned(),
        },
        val: "test".to_owned(),
    };
    let mut a: AnnoStorage<NodeID> = AnnoStorage::new();
    a.insert(1, test_anno.clone());
    a.insert(1, test_anno.clone());
    a.insert(2, test_anno.clone());
    a.insert(3, test_anno);

    assert_eq!(3, a.number_of_annotations());
    assert_eq!(3, a.by_container.len());
    assert_eq!(1, a.by_anno.len());
    assert_eq!(1, a.anno_keys.len());

    assert_eq!(
        "test",
        a.get_value_for_item(
            &3,
            &AnnoKey {
                name: "anno1".to_owned(),
                ns: "annis".to_owned()
            }
        )
        .unwrap()
    );
}

#[test]
fn get_all_for_node() {
    let test_anno1 = Annotation {
        key: AnnoKey {
            name: "anno1".to_owned(),
            ns: "annis1".to_owned(),
        },
        val: "test".to_owned(),
    };
    let test_anno2 = Annotation {
        key: AnnoKey {
            name: "anno2".to_owned(),
            ns: "annis2".to_owned(),
        },
        val: "test".to_owned(),
    };
    let test_anno3 = Annotation {
        key: AnnoKey {
            name: "anno3".to_owned(),
            ns: "annis1".to_owned(),
        },
        val: "test".to_owned(),
    };

    let mut a: AnnoStorage<NodeID> = AnnoStorage::new();
    a.insert(1, test_anno1.clone());
    a.insert(1, test_anno2.clone());
    a.insert(1, test_anno3.clone());

    assert_eq!(3, a.number_of_annotations());

    let all = a.get_annotations_for_item(&1);
    assert_eq!(3, all.len());

    assert_eq!(test_anno1, all[0]);
    assert_eq!(test_anno2, all[1]);
    assert_eq!(test_anno3, all[2]);
}

#[test]
fn remove() {
    let test_anno = Annotation {
        key: AnnoKey {
            name: "anno1".to_owned(),
            ns: "annis1".to_owned(),
        },
        val: "test".to_owned(),
    };
    let mut a: AnnoStorage<NodeID> = AnnoStorage::new();
    a.insert(1, test_anno.clone());

    assert_eq!(1, a.number_of_annotations());
    assert_eq!(1, a.by_container.len());
    assert_eq!(1, a.by_anno.len());
    assert_eq!(1, a.anno_key_sizes.len());
    assert_eq!(&1, a.anno_key_sizes.get(&test_anno.key).unwrap());

    a.remove_annotation_for_item(&1, &test_anno.key);

    assert_eq!(0, a.number_of_annotations());
    assert_eq!(0, a.by_container.len());
    assert_eq!(0, a.by_anno.len());
    assert_eq!(&0, a.anno_key_sizes.get(&test_anno.key).unwrap_or(&0));
}
