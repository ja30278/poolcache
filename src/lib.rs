//! A PoolCache is a hybrid LFU cache and object pool, that allows
//! for caching behavior with the possibility of reusing object
//! rather than dropping them from the cache automatically.
//!
//! # Examples
//! ```
//! use poolcache::PoolCache;
//!
//! // Create a new pool cache with a maximum 'heat' of 4.
//! // Larger maxium heat values make popular values more resistent
//! // to being reused, but at the cost of increasing the potential
//! // work required to find a re-usable entry.
//! let mut cache : PoolCache<u64, Vec<u8>> = PoolCache::new(4);
//!
//! // Caches are empty until you populate them..`insert` adds a 
//! // new value associated with a key.
//! cache.insert(1, Vec::new());
//!
//! // `cache` now contains a single vector, associated with the key
//! // `1`, which can be retrieved with `get`
//! {
//!     let vecref : &Vec<u8> = cache.get(&1).unwrap();
//! }
//!
//! // You can also add values that aren't associated with any key with
//! // `put`. These newly added values will be used to satisfy `take`
//! // requests before evicting objects with keys.
//! cache.put(Vec::new());
//!
//! // You can get an owned object from the pool using `take`. This will
//! // use any free objects (if available), or evict the least `hot`
//! // key from the cache, and return its value.
//! let ownedvec : Vec<u8> = cache.take().unwrap();
//! ```
//!

use std::cell::Cell;
use std::cmp;
use std::collections::{BTreeMap,VecDeque};

struct CacheEntry<Value> {
    val: Value,
    heat: Cell<u64>,
}

impl<Value> CacheEntry<Value> {
    fn new(val: Value) -> CacheEntry<Value> {
        CacheEntry{val: val, heat: Cell::new(1)}
    }

    fn inc(&self, max_heat: u64) -> u64 {
        self.heat.set(cmp::min(self.heat.get() + 1, max_heat));
        self.heat.get()
    }

    fn dec(&self) -> u64 {
        self.heat.set(cmp::max(self.heat.get() - 1, 0));
        self.heat.get()
    }
}

pub struct PoolCache<Key, Value> {
    cache: BTreeMap<Key, CacheEntry<Value>>,
    freelist: VecDeque<Value>,
    clock: VecDeque<Key>,
    max_heat: u64,
}

impl<Key, Value> PoolCache<Key, Value>
    where Key: PartialOrd + Ord + Clone {

        /// Create a new PoolCache where the maximum heat of a value
        /// is limited to `max_heat`.
        pub fn new(max_heat: u64) -> PoolCache<Key, Value> {
            PoolCache{
                cache: BTreeMap::new(),
                freelist: VecDeque::new(),
                clock: VecDeque::new(),
                max_heat: max_heat}
        }

        /// Returns `true` if the given key is present in the cache.
        pub fn contains_key(&self, key: &Key) -> bool {
            self.cache.contains_key(key)
        }

        /// Returns a reference to the value associated with `key`, or `None`
        /// if the key is not present in the cache.
        pub fn get(&self, key: &Key) -> Option<&Value> {
            self.cache.get(key).and_then(|entry| {
                entry.inc(self.max_heat);
                Some(&entry.val)
            })
        }

        /// Add a new object to the pool, not associated with any
        /// key. This will become available to any callers of `take`. 
        pub fn put(&mut self, val: Value) {
            self.freelist.push_back(val)
        }

        /// Insert `val` into the map associated with `key`. Any previous
        /// entry for `key` will be replaced, and the old value will become
        /// available for new callers of `take`.
        pub fn insert(&mut self, key: Key, val: Value) {
            let mut found_entry = false;
            if let Some(old_entry) = self.cache.remove(&key) {
                self.freelist.push_back(old_entry.val);
                found_entry = true;
            }
            if !found_entry {
                self.clock.push_back(key.clone());
            }
            self.cache.insert(key, CacheEntry::new(val));
        }

        /// Take returns an object from the pool, evicting the least-used
        /// cached key if necessary. Returns `None` only if the PoolCache
        /// contains no items.
        pub fn take(&mut self) -> Option<Value> {
            if let Some(val) = self.freelist.pop_front() {
                return Some(val);
            }
            // cache is empty.
            if self.clock.is_empty() {
                return None;
            }
            // loop over the elements in `clock`, decrementing heat until
            // we find an eligible value to evict.
            loop {
                let key = self.clock.pop_front().unwrap();
                let heat = self.cache.get(&key).unwrap().dec();
                if heat == 0 {
                    // eligible element.
                    return Some(self.cache.remove(&key).unwrap().val);
                }
                // non-zero heat, keep looping.
                self.clock.push_back(key);
            }
        }
}


#[cfg(test)]
mod test {
    #[test]
    fn basic() {
        // can't take from an empty cache.
        let mut cache: super::PoolCache<u64, String> = super::PoolCache::new(5);
        assert_eq!(None, cache.take());

        // adding an object to the cache not associated with a value,
        // and returning it via 'take'.
        cache.put(String::from("foo"));
        assert_eq!(Some(String::from("foo")), cache.take());

        // Since we only added one value (and then took it), the
        // cache is empty again.
        assert_eq!(None, cache.take());

        // Add a keyed value, and retrieve it a few times.
        cache.insert(1, String::from("bar"));
        assert_eq!("bar", cache.get(&1).unwrap());
        assert_eq!("bar", cache.get(&1).unwrap());
        assert_eq!("bar", cache.get(&1).unwrap());

        // Add a second value, and retrieve it only once.
        cache.insert(2, String::from("baz"));
        assert_eq!("baz", cache.get(&2).unwrap());


        // taking a value returns 'baz', since it only has one use.
        assert_eq!(Some(String::from("baz")), cache.take());

        // Now that we've taken it's value, the key '2' is no longer
        // in the cache.
        assert_eq!(None, cache.get(&2));

        // '1' is still in the cache
        assert!(cache.contains_key(&1));

        // Replace it's value (currently 'bar') with a new value.
        cache.insert(1, String::from("newbar"));
        assert_eq!("newbar", cache.get(&1).unwrap());

        // The old value ('bar') is moved to the freelist, and is
        // returned to the next caller of `take`
        assert_eq!(Some(String::from("bar")), cache.take());

        // A final `take` removes the last value in the pool 
        // (currently keyed to '1')
        assert_eq!(Some(String::from("newbar")), cache.take());

        // leaving the cache empty.
        assert_eq!(None, cache.take());
    }
}
