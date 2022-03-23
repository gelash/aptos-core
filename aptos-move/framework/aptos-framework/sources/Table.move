/// This module provides a temporary solution for tables by providing a layer on top of Vector
module AptosFramework::Table {
    use Std::Errors;
    use Std::Option;
    use Std::Vector;

    const EKEY_ALREADY_EXISTS: u64 = 0;
    const EKEY_NOT_FOUND: u64 = 1;

    struct Table<Key: drop + store, Value: drop + store> has drop, store {
      data: vector<TableElement<Key, Value>>,
    }

    struct TableElement<Key: drop + store, Value: drop + store> has drop, store {
        key: Key,
        value: Value,
    }

    public fun create<Key: drop + store, Value: drop + store>(): Table<Key, Value> {
        Table {
            data: Vector::empty(),
        }
    }

    public fun borrow<Key: drop + store, Value: drop + store>(
        table: &Table<Key, Value>,
        key: &Key,
    ): &Value {
        let maybe_idx = find(table, key);
        assert!(Option::is_some(&maybe_idx), Errors::invalid_argument(EKEY_NOT_FOUND));
        let idx = *Option::borrow(&maybe_idx);
        &Vector::borrow(&table.data, idx).value
    }

    public fun borrow_mut<Key: drop + store, Value: drop + store>(
        table: &mut Table<Key, Value>,
        key: &Key,
    ): &mut Value {
        let maybe_idx = find(table, key);
        assert!(Option::is_some(&maybe_idx), Errors::invalid_argument(EKEY_NOT_FOUND));
        let idx = *Option::borrow(&maybe_idx);
        &mut Vector::borrow_mut(&mut table.data, idx).value
    }

    public fun contains_key<Key: drop + store, Value: drop + store>(
        table: &Table<Key, Value>,
        key: &Key,
    ): bool {
        Option::is_some(&find(table, key))
    }

    public fun insert<Key: drop + store, Value: drop + store>(
        table: &mut Table<Key, Value>,
        key: Key,
        value: Value,
    ) {
        let maybe_idx = find(table, &key);
        assert!(Option::is_none(&maybe_idx), Errors::invalid_argument(EKEY_ALREADY_EXISTS));
        Vector::push_back(&mut table.data, TableElement { key, value });
    }

    public fun remove<Key: drop + store, Value: drop + store>(
        table: &mut Table<Key, Value>,
        key: &Key,
    ): Value {
        let maybe_idx = find(table, key);
        assert!(Option::is_some(&maybe_idx), Errors::invalid_argument(EKEY_NOT_FOUND));
        let idx = *Option::borrow(&maybe_idx);
        let TableElement { key: _key, value } = Vector::swap_remove(&mut table.data, idx);
        value
    }

    fun find<Key: drop + store, Value: drop + store>(
        table: &Table<Key, Value>,
        key: &Key,
    ): Option::Option<u64> {
        let size = Vector::length(&table.data);
        let idx = 0;
        while (idx < size) {
            if (&Vector::borrow(&table.data, idx).key == key) {
                return Option::some(idx)
            };
            idx = idx + 1
        };

        Option::none()
    }

    #[test]
    public fun add_remove_many() {
        let table = create<u64, u64>();

        assert!(!contains_key(&table, &3), 0);
        insert(&mut table, 3, 1);
        assert!(contains_key(&table, &3), 1);
        assert!(borrow(&table, &3) == &1, 2);
        *borrow_mut(&mut table, &3) = 2;
        assert!(borrow(&table, &3) == &2, 3);

        assert!(!contains_key(&table, &2), 4);
        insert(&mut table, 2, 5);
        assert!(contains_key(&table, &2), 5);
        assert!(borrow(&table, &2) == &5, 6);
        *borrow_mut(&mut table, &2) = 9;
        assert!(borrow(&table, &2) == &9, 7);

        remove(&mut table, &2);
        assert!(!contains_key(&table, &2), 8);
        assert!(borrow(&table, &3) == &2, 9);

        remove(&mut table, &3);
        assert!(!contains_key(&table, &3), 10);
    }

    #[test]
    #[expected_failure]
    public fun insert_twice() {
        let table = create<u64, u64>();
        insert(&mut table, 3, 1);
        insert(&mut table, 3, 1);
    }

    #[test]
    #[expected_failure]
    public fun remove_twice() {
        let table = create<u64, u64>();
        insert(&mut table, 3, 1);
        remove(&mut table, &3);
        remove(&mut table, &3);
    }
}
