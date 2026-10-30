//! 有界内存 LRU：精确 key 命中，按逻辑槽位去重，按字节淘汰。

/// 缓存 key：`same_slot` 判断同一逻辑条目的不同版本（如 mtime 变化）。
pub(super) trait CacheKey: PartialEq {
    fn same_slot(&self, other: &Self) -> bool;
}

struct Entry<K, V> {
    key: K,
    value: V,
    bytes: usize,
}

pub(super) struct LruCache<K: CacheKey, V> {
    entries: Vec<Entry<K, V>>,
    total: usize,
    max_bytes: usize,
}

impl<K: CacheKey, V: Clone> LruCache<K, V> {
    pub(super) fn new(max_bytes: usize) -> Self {
        LruCache {
            entries: Vec::new(),
            total: 0,
            max_bytes,
        }
    }

    /// 精确 key 命中（版本字段也参与）；命中后移到队首。
    pub(super) fn get(&mut self, key: &K) -> Option<V> {
        let pos = self.entries.iter().position(|e| e.key == *key)?;
        let entry = self.entries.remove(pos);
        let value = entry.value.clone();
        self.entries.insert(0, entry);
        Some(value)
    }

    /// 按 `same_slot` 去重；插入队首；超上限从队尾淘汰，至少保留一条。
    pub(super) fn insert(&mut self, key: K, value: V, bytes: usize) {
        if let Some(pos) = self.entries.iter().position(|e| e.key.same_slot(&key)) {
            let old = self.entries.remove(pos);
            self.total = self.total.saturating_sub(old.bytes);
        }
        self.total = self.total.saturating_add(bytes);
        self.entries.insert(0, Entry { key, value, bytes });
        while self.total > self.max_bytes && self.entries.len() > 1 {
            let Some(old) = self.entries.pop() else { break };
            self.total = self.total.saturating_sub(old.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq)]
    struct TestKey {
        slot: u8,
        version: u8,
    }

    impl CacheKey for TestKey {
        fn same_slot(&self, other: &Self) -> bool {
            self.slot == other.slot
        }
    }

    fn key(slot: u8, version: u8) -> TestKey {
        TestKey { slot, version }
    }

    #[test]
    fn get_requires_exact_version() {
        let mut cache = LruCache::new(1024);
        cache.insert(key(1, 1), Arc::new(vec![1u8]), 1);
        assert!(cache.get(&key(1, 1)).is_some());
        assert!(cache.get(&key(1, 2)).is_none(), "版本变化必须 miss");
    }

    #[test]
    fn insert_replaces_same_slot() {
        let mut cache = LruCache::new(1024);
        cache.insert(key(1, 1), Arc::new(vec![1u8]), 1);
        cache.insert(key(1, 2), Arc::new(vec![2u8]), 1);

        assert!(cache.get(&key(1, 1)).is_none());
        assert_eq!(cache.get(&key(1, 2)).unwrap().as_slice(), &[2]);
        assert_eq!(cache.entries.len(), 1, "同槽位旧版本必须被替换");
        assert_eq!(cache.total, 1);
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = LruCache::new(20);
        cache.insert(key(1, 1), Arc::new(vec![1u8]), 10);
        cache.insert(key(2, 1), Arc::new(vec![2u8]), 10);
        assert!(cache.get(&key(1, 1)).is_some(), "触碰 slot 1");

        cache.insert(key(3, 1), Arc::new(vec![3u8]), 10);
        assert!(cache.get(&key(1, 1)).is_some(), "最近使用的应保留");
        assert!(cache.get(&key(2, 1)).is_none(), "最久未使用的应淘汰");
        assert!(cache.get(&key(3, 1)).is_some());
        assert_eq!(cache.total, 20);
    }

    #[test]
    fn keeps_oversized_entry() {
        let mut cache = LruCache::new(4);
        cache.insert(key(1, 1), Arc::new(vec![1u8]), 10);
        assert!(
            cache.get(&key(1, 1)).is_some(),
            "单个超大条目不能插入即被淘汰"
        );
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn get_does_not_change_bytes() {
        let mut cache = LruCache::new(1024);
        cache.insert(key(1, 1), Arc::new(vec![1u8]), 7);
        cache.get(&key(1, 1));
        assert_eq!(cache.total, 7);
    }
}
