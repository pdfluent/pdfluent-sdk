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
/// The cache is bounded by entry COUNT rather than total bytes. This is a
/// deliberate Sprint-A choice: the cache is per-`PdfDocument` and lives only as
/// long as that document, individual decoded-image size is already bounded in
/// practice by the render target dimensions (and by `ProcessingLimits::
/// max_image_pixels` when processing limits are configured), and a small fixed
/// count keeps eviction O(1)-ish without per-variant byte accounting. A
/// byte-budgeted policy (summing `DecodedImageXObject` sizes) is the natural
/// follow-up if profiling shows image-heavy documents exceeding the memory
/// envelope; until then 32 is a conservative working-set size for repeated
/// images (logos, headers/footers) across pages.
const IMAGE_CACHE_CAPACITY: usize = 32;

struct ImageCache {
    map: HashMap<u128, Box<dyn Any + Send + Sync>>,
    order: VecDeque<u128>,
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

    pub(crate) fn get_or_insert_image<T: Clone + Send + Sync + 'static>(
        &self,
        id: u128,
        f: impl FnOnce() -> Option<T>,
    ) -> Option<T> {
        let mut locked = self.images.lock().unwrap();
        if locked.map.contains_key(&id) {
            if let Some(pos) = locked.order.iter().position(|&x| x == id) {
                locked.order.remove(pos);
            }
            locked.order.push_back(id);
            let val = locked.map.get(&id).unwrap();
            return val.downcast_ref::<T>().cloned();
        }

        drop(locked);
        let val = f();
        if let Some(ref v) = val {
            let mut locked = self.images.lock().unwrap();
            locked
                .map
                .insert(id, Box::new(v.clone()) as Box<dyn Any + Send + Sync>);
            if let Some(pos) = locked.order.iter().position(|&x| x == id) {
                locked.order.remove(pos);
            }
            locked.order.push_back(id);

            while locked.order.len() > IMAGE_CACHE_CAPACITY {
                if let Some(oldest_id) = locked.order.pop_front() {
                    locked.map.remove(&oldest_id);
                }
            }
        }
        val
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

    #[test]
    fn test_image_cache_lru_eviction() {
        let cache = Cache::new();

        // 1. Insert 32 unique items
        for i in 0..32 {
            let val = cache.get_or_insert_image(i as u128, || Some(i));
            assert_eq!(val, Some(i));
        }

        // Verify all 32 items are still there
        for i in 0..32 {
            let cached = cache.get_or_insert_image::<i32>(i as u128, || None);
            assert_eq!(cached, Some(i));
        }

        // 2. Access key 0 to make it recently used (brings it to the back)
        let first_accessed = cache.get_or_insert_image::<i32>(0, || None);
        assert_eq!(first_accessed, Some(0));

        // 3. Insert a new item (key 32)
        // This should cause an eviction. The oldest item (which is key 1, since key 0 was recently used) should be evicted.
        let val32 = cache.get_or_insert_image(32, || Some(32));
        assert_eq!(val32, Some(32));

        // Key 1 should be None (evicted)
        let evicted1 = cache.get_or_insert_image::<i32>(1, || None);
        assert_eq!(evicted1, None);

        // Key 0 should still be Some(0) (since we touched it)
        let kept0 = cache.get_or_insert_image::<i32>(0, || None);
        assert_eq!(kept0, Some(0));
    }
}
