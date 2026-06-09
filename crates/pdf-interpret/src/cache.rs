use crate::util::hash128;
use kurbo::Affine;
use pdf_syntax::object::{Array, Dict, MaybeRef, Name, Null, ObjRef, Object, Stream};
use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

type CacheMap = HashMap<u128, Option<Box<dyn Any + Send + Sync>>>;

/// Maximum number of decoded images retained by the document-level image cache.
///
/// The cache is bounded by BOTH entry count (this constant) and total decoded
/// bytes ([`IMAGE_CACHE_MAX_BYTES`]); an insertion evicts least-recently-used
/// entries until both bounds hold. The count cap catches "many small images"
/// (logos, headers/footers repeated across pages); the byte cap catches "few
/// but huge images" that would otherwise pass the count cap while consuming
/// hundreds of MB. The cache is per-`PdfDocument` and lives only as long as
/// that document.
const IMAGE_CACHE_CAPACITY: usize = 32;

/// Maximum total decoded-image bytes retained by the document-level image cache.
///
/// 256 MiB is a conservative working-set ceiling for image-heavy documents: it
/// comfortably holds typical repeated assets while bounding peak transient
/// memory for pathological inputs (e.g. a handful of full-page scans). An entry
/// larger than the whole budget is returned to the caller but not retained.
const IMAGE_CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;

struct ImageCache {
    /// Maps a cache key to the decoded value and its accounted byte weight.
    map: HashMap<u128, (Box<dyn Any + Send + Sync>, usize)>,
    /// Least-recently-used order; front = oldest, back = most recently used.
    order: VecDeque<u128>,
    /// Sum of the byte weights of all entries currently in `map`.
    total_bytes: usize,
}

/// A cache to store decoded images and other objects to avoid parsing/decoding them repeatedly.
#[derive(Clone)]
pub struct Cache {
    generic: Arc<Mutex<CacheMap>>,
    images: Arc<Mutex<ImageCache>>,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    /// Create a new, empty cache.
    pub fn new() -> Self {
        Self {
            generic: Arc::new(Mutex::new(HashMap::new())),
            images: Arc::new(Mutex::new(ImageCache {
                map: HashMap::new(),
                order: VecDeque::new(),
                total_bytes: 0,
            })),
        }
    }

    pub(crate) fn get_or_insert_with<T: Clone + Send + Sync + 'static>(
        &self,
        id: u128,
        f: impl FnOnce() -> Option<T>,
    ) -> Option<T> {
        let mut locked = self.generic.lock().unwrap();

        // We can't use `get_or_insert_with` here, because if the closure makes another access to the
        // cache, we end up with a deadlock.
        match locked.entry(id) {
            Entry::Occupied(o) => o
                .get()
                .as_ref()
                .and_then(|val| val.downcast_ref::<T>().cloned()),
            Entry::Vacant(_) => {
                drop(locked);
                let val = f();
                self.generic.lock().unwrap().insert(
                    id,
                    val.clone()
                        .map(|val| Box::new(val) as Box<dyn Any + Send + Sync>),
                );

                val
            }
        }
    }

    /// Look up a decoded image by `id`, or compute it with `f` and cache it.
    ///
    /// `weight` returns the byte size of a freshly-computed value; it is used to
    /// keep the cache within [`IMAGE_CACHE_MAX_BYTES`]. The cache is also bounded
    /// to [`IMAGE_CACHE_CAPACITY`] entries. After an insertion, least-recently-
    /// used entries are evicted until both bounds hold (a single value larger
    /// than the whole budget is returned but not retained).
    pub(crate) fn get_or_insert_image<T: Clone + Send + Sync + 'static>(
        &self,
        id: u128,
        f: impl FnOnce() -> Option<T>,
        weight: impl FnOnce(&T) -> usize,
    ) -> Option<T> {
        let mut locked = self.images.lock().unwrap();
        if locked.map.contains_key(&id) {
            if let Some(pos) = locked.order.iter().position(|&x| x == id) {
                locked.order.remove(pos);
            }
            locked.order.push_back(id);
            let (val, _) = locked.map.get(&id).unwrap();
            return val.downcast_ref::<T>().cloned();
        }

        drop(locked);
        let val = f();
        if let Some(ref v) = val {
            let w = weight(v);
            let mut locked = self.images.lock().unwrap();
            // A repeated key would otherwise leak its old weight; subtract it.
            if let Some((_, old_w)) = locked
                .map
                .insert(id, (Box::new(v.clone()) as Box<dyn Any + Send + Sync>, w))
            {
                locked.total_bytes = locked.total_bytes.saturating_sub(old_w);
                if let Some(pos) = locked.order.iter().position(|&x| x == id) {
                    locked.order.remove(pos);
                }
            }
            locked.total_bytes += w;
            locked.order.push_back(id);

            // Evict least-recently-used entries until both the count and the
            // byte budget are satisfied.
            while !locked.order.is_empty()
                && (locked.order.len() > IMAGE_CACHE_CAPACITY
                    || locked.total_bytes > IMAGE_CACHE_MAX_BYTES)
            {
                if let Some(oldest_id) = locked.order.pop_front()
                    && let Some((_, evicted_w)) = locked.map.remove(&oldest_id)
                {
                    locked.total_bytes = locked.total_bytes.saturating_sub(evicted_w);
                }
            }
        }
        val
    }

    /// Current image-cache occupancy as `(entry_count, total_bytes)`.
    ///
    /// Exposed for measurement and tests; both values are always within
    /// [`IMAGE_CACHE_CAPACITY`] and [`IMAGE_CACHE_MAX_BYTES`] respectively after
    /// any completed `get_or_insert_image` call.
    #[cfg(test)]
    pub(crate) fn image_cache_stats(&self) -> (usize, usize) {
        let locked = self.images.lock().unwrap();
        (locked.map.len(), locked.total_bytes)
    }
}

/// A trait for objects that can generate a unique cache key.
pub trait CacheKey {
    /// Returns the cache key for this object.
    fn cache_key(&self) -> u128;
}

impl<T: CacheKey, U: CacheKey> CacheKey for (T, U) {
    fn cache_key(&self) -> u128 {
        hash128(&(self.0.cache_key(), self.1.cache_key()))
    }
}

impl CacheKey for Dict<'_> {
    fn cache_key(&self) -> u128 {
        hash128(self.data())
    }
}

impl CacheKey for Stream<'_> {
    fn cache_key(&self) -> u128 {
        self.dict().cache_key()
    }
}

impl CacheKey for Null {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl CacheKey for bool {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl CacheKey for pdf_syntax::object::Number {
    fn cache_key(&self) -> u128 {
        hash128(&self.as_f64().to_bits())
    }
}

impl CacheKey for pdf_syntax::object::String {
    fn cache_key(&self) -> u128 {
        hash128(self.as_ref())
    }
}

impl CacheKey for Name {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl CacheKey for Array<'_> {
    fn cache_key(&self) -> u128 {
        hash128(self.data())
    }
}

impl CacheKey for Object<'_> {
    fn cache_key(&self) -> u128 {
        match self {
            Object::Null(n) => n.cache_key(),
            Object::Boolean(b) => b.cache_key(),
            Object::Number(n) => n.cache_key(),
            Object::String(s) => s.cache_key(),
            Object::Name(n) => n.cache_key(),
            Object::Dict(d) => d.cache_key(),
            Object::Array(a) => a.cache_key(),
            Object::Stream(s) => s.cache_key(),
        }
    }
}

impl CacheKey for ObjRef {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl<T: CacheKey> CacheKey for MaybeRef<T> {
    fn cache_key(&self) -> u128 {
        match self {
            Self::Ref(r) => r.cache_key(),
            Self::NotRef(o) => o.cache_key(),
        }
    }
}

impl CacheKey for Affine {
    fn cache_key(&self) -> u128 {
        let c = self.as_coeffs();
        hash128(&[
            c[0].to_bits(),
            c[1].to_bits(),
            c[2].to_bits(),
            c[3].to_bits(),
            c[4].to_bits(),
            c[5].to_bits(),
        ])
    }
}

impl CacheKey for u128 {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generic_cache() {
        let cache = Cache::new();
        let val = cache.get_or_insert_with(12345, || Some("hello".to_string()));
        assert_eq!(val, Some("hello".to_string()));

        let cached = cache.get_or_insert_with(12345, || Some("world".to_string()));
        assert_eq!(cached, Some("hello".to_string()));
    }

    // A small weight; keeps these count-focused cases far below the byte budget.
    fn tiny(_: &i32) -> usize {
        4
    }

    #[test]
    fn test_image_cache_lru_eviction() {
        let cache = Cache::new();

        // 1. Insert 32 unique items
        for i in 0..32 {
            let val = cache.get_or_insert_image(i as u128, || Some(i), tiny);
            assert_eq!(val, Some(i));
        }

        // Verify all 32 items are still there
        for i in 0..32 {
            let cached = cache.get_or_insert_image::<i32>(i as u128, || None, tiny);
            assert_eq!(cached, Some(i));
        }

        // 2. Access key 0 to make it recently used (brings it to the back)
        let first_accessed = cache.get_or_insert_image::<i32>(0, || None, tiny);
        assert_eq!(first_accessed, Some(0));

        // 3. Insert a new item (key 32). This evicts the oldest entry, which is
        //    key 1 (key 0 was just touched).
        let val32 = cache.get_or_insert_image(32, || Some(32), tiny);
        assert_eq!(val32, Some(32));

        // Key 1 should be None (evicted)
        let evicted1 = cache.get_or_insert_image::<i32>(1, || None, tiny);
        assert_eq!(evicted1, None);

        // Key 0 should still be Some(0) (since we touched it)
        let kept0 = cache.get_or_insert_image::<i32>(0, || None, tiny);
        assert_eq!(kept0, Some(0));
    }

    #[test]
    fn test_image_cache_evicts_by_byte_budget() {
        let cache = Cache::new();
        let big = 100 * 1024 * 1024; // 100 MiB each; three => 300 MiB > 256 MiB.

        cache.get_or_insert_image(1u128, || Some(1i32), |_| big);
        cache.get_or_insert_image(2u128, || Some(2i32), |_| big);
        cache.get_or_insert_image(3u128, || Some(3i32), |_| big);

        // Only three entries (far under the 32 count cap), but their bytes exceed
        // the budget, so the least-recently-used (key 1) is evicted.
        let (count, bytes) = cache.image_cache_stats();
        assert_eq!(count, 2, "byte budget should drop the cache to two entries");
        assert!(
            bytes <= IMAGE_CACHE_MAX_BYTES,
            "total bytes must stay within budget"
        );
        assert_eq!(
            cache.get_or_insert_image::<i32>(1, || None, |_| 0),
            None,
            "oldest entry evicted by byte budget"
        );
        assert_eq!(cache.get_or_insert_image::<i32>(2, || None, |_| 0), Some(2));
        assert_eq!(cache.get_or_insert_image::<i32>(3, || None, |_| 0), Some(3));
    }

    #[test]
    fn test_image_cache_byte_budget_respects_lru_touch() {
        let cache = Cache::new();
        let big = 100 * 1024 * 1024;

        cache.get_or_insert_image(1u128, || Some(1i32), |_| big);
        cache.get_or_insert_image(2u128, || Some(2i32), |_| big);
        // Touch key 1 so it becomes most-recently-used.
        assert_eq!(cache.get_or_insert_image::<i32>(1, || None, |_| 0), Some(1));
        // Inserting key 3 pushes bytes over budget; the LRU victim is now key 2.
        cache.get_or_insert_image(3u128, || Some(3i32), |_| big);

        assert_eq!(
            cache.get_or_insert_image::<i32>(2, || None, |_| 0),
            None,
            "least-recently-used key 2 evicted, not the touched key 1"
        );
        assert_eq!(cache.get_or_insert_image::<i32>(1, || None, |_| 0), Some(1));
        assert_eq!(cache.get_or_insert_image::<i32>(3, || None, |_| 0), Some(3));
    }

    #[test]
    fn test_image_cache_oversized_item_not_retained() {
        let cache = Cache::new();
        let huge = IMAGE_CACHE_MAX_BYTES + 1; // larger than the whole budget

        // The value is still returned to the caller for this call...
        assert_eq!(
            cache.get_or_insert_image(1u128, || Some(1i32), |_| huge),
            Some(1)
        );
        // ...but it is evicted immediately, so the cache retains nothing.
        let (count, bytes) = cache.image_cache_stats();
        assert_eq!(count, 0, "an item larger than the budget is not retained");
        assert_eq!(bytes, 0);
        assert_eq!(cache.get_or_insert_image::<i32>(1, || None, |_| 0), None);
    }

    #[test]
    fn test_image_cache_stats_stay_within_both_bounds() {
        let cache = Cache::new();
        // 100 small (1 KiB) items: the count cap bounds entries to 32; bytes
        // stay tiny. This is the measurement asserting both bounds hold.
        for i in 0..100u128 {
            cache.get_or_insert_image(i, || Some(i as i32), |_| 1024);
        }
        let (count, bytes) = cache.image_cache_stats();
        assert_eq!(count, IMAGE_CACHE_CAPACITY, "count cap holds at 32 entries");
        assert_eq!(
            bytes,
            IMAGE_CACHE_CAPACITY * 1024,
            "32 retained * 1 KiB each"
        );
        assert!(bytes <= IMAGE_CACHE_MAX_BYTES);
    }
}
