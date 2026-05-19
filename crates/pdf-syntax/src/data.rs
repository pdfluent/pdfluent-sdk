use crate::object::ObjectIdentifier;
use crate::object::Stream;
use crate::reader::ReaderContext;
use crate::sync::HashMap;
use crate::sync::{Arc, Mutex, MutexExt};
use crate::util::SegmentList;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter};

/// Parsed `(object_number, absolute_byte_offset)` table for an object stream
/// (PDF 1.5 compressed `/ObjStm`).
///
/// This is a value type — once produced from the stream header it never
/// changes for the lifetime of the source `Data`.
pub(crate) type ObjectStreamOffsets = Vec<(u32, usize)>;

/// A container for the bytes of a PDF file.
#[derive(Clone)]
pub struct PdfData {
    #[cfg(feature = "std")]
    inner: Arc<dyn AsRef<[u8]> + Send + Sync>,
    #[cfg(not(feature = "std"))]
    inner: Arc<dyn AsRef<[u8]>>,
}

impl Debug for PdfData {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "PdfData {{ ... }}")
    }
}

impl AsRef<[u8]> for PdfData {
    fn as_ref(&self) -> &[u8] {
        (*self.inner).as_ref()
    }
}

#[cfg(feature = "std")]
impl<T: AsRef<[u8]> + Send + Sync + 'static> From<Arc<T>> for PdfData {
    fn from(data: Arc<T>) -> Self {
        Self { inner: data }
    }
}

#[cfg(not(feature = "std"))]
impl<T: AsRef<[u8]> + 'static> From<Arc<T>> for PdfData {
    fn from(data: Arc<T>) -> Self {
        Self { inner: data }
    }
}

impl From<Vec<u8>> for PdfData {
    fn from(data: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(data),
        }
    }
}

/// A structure for storing the data of the PDF.
// To explain further: This crate uses a zero-parse approach, meaning that objects like
// dictionaries or arrays always store the underlying data and parse objects lazily as needed,
// instead of allocating the data and storing it in an owned way. However, the problem is that
// not all data is readily available in the original data of the PDF: Objects can also be
// stored in an object streams, in which case we first need to decode the stream before we can
// access the data.
//
// The purpose of `Data` is to allow us to access the original data as well as maybe decoded data
// by faking the same lifetime, so that we don't run into lifetime issues when dealing with
// PDF objects that actually stem from different data sources.
pub(crate) struct Data {
    data: PdfData,
    // 32 segments are more than enough as we can't have more objects than this.
    decoded: SegmentList<Option<Vec<u8>>, 32>,
    map: Mutex<HashMap<ObjectIdentifier, usize>>,
    // QF2-B: cache of parsed `(obj_num, abs_offset)` index tables, keyed by the
    // object stream's own `ObjectIdentifier`. Lives for the lifetime of the
    // owning document and is dropped together with it — there is no
    // cross-document leakage and the underlying byte slice (`data`) is
    // immutable for the same lifetime, so cache entries can never become
    // stale.
    object_stream_offsets: Mutex<HashMap<ObjectIdentifier, Arc<ObjectStreamOffsets>>>,
}

impl Debug for Data {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Data {{ ... }}")
    }
}

impl Data {
    /// Create a new `Data` structure.
    pub(crate) fn new(data: PdfData) -> Self {
        Self {
            data,
            decoded: SegmentList::new(),
            map: Mutex::new(HashMap::new()),
            object_stream_offsets: Mutex::new(HashMap::new()),
        }
    }

    /// Get access to the original data of the PDF.
    pub(crate) fn get(&self) -> &PdfData {
        &self.data
    }

    /// Look up the cached parsed `(obj_num, abs_offset)` index table for the
    /// object stream `id`, computing it via `parse` on cache miss.
    ///
    /// Returns `None` if `parse` returns `None` (and does **not** populate
    /// the cache in that case, so a later attempt can retry).
    ///
    /// # Invariants
    ///
    /// - Cache scope: per-`Data` (i.e. per source byte slice). Two distinct
    ///   `Pdf` instances do not share an offsets cache.
    /// - Cache lifetime: bounded by the owning `Pdf` document. Dropping the
    ///   `Pdf` (and therefore the `Data`) frees all entries.
    /// - Mutation safety: `PdfData` wraps an immutable `Arc<dyn AsRef<[u8]>>`,
    ///   so the byte slice cannot change underneath a cache entry. There is
    ///   no need for explicit invalidation.
    pub(crate) fn get_object_stream_offsets_or_init<F>(
        &self,
        id: ObjectIdentifier,
        parse: F,
    ) -> Option<Arc<ObjectStreamOffsets>>
    where
        F: FnOnce() -> Option<ObjectStreamOffsets>,
    {
        if let Some(hit) = self.object_stream_offsets.get().get(&id).cloned() {
            return Some(hit);
        }

        // Parse outside the lock to avoid holding it during potentially
        // expensive work.
        let parsed = Arc::new(parse()?);

        // Insert-if-absent: another thread may have populated the entry while
        // we parsed. Whichever entry is in the map first wins, and we hand
        // that one back so callers always observe the same parsed table.
        let mut locked = self.object_stream_offsets.get();
        Some(locked.entry(id).or_insert(parsed).clone())
    }

    /// Number of cached object-stream offset tables. Test-only.
    #[cfg(test)]
    pub(crate) fn object_stream_offsets_cache_len(&self) -> usize {
        self.object_stream_offsets.get().len()
    }

    /// Get access to the data of a decoded object stream.
    pub(crate) fn get_with(&self, id: ObjectIdentifier, ctx: &ReaderContext<'_>) -> Option<&[u8]> {
        if let Some(&idx) = self.map.get().get(&id) {
            self.decoded.get(idx)?.as_deref()
        } else {
            // Block scope to keep the lock short-lived.
            let idx = {
                let mut locked = self.map.get();
                let idx = locked.len();
                locked.insert(id, idx);
                idx
            };
            self.decoded
                .get_or_init(idx, || {
                    let stream = ctx.xref().get_with::<Stream<'_>>(id, ctx)?;
                    stream.decoded().ok()
                })
                .as_deref()
        }
    }
}

#[cfg(test)]
mod tests {
    //! QF2-B — unit tests for the per-document parsed ObjectStream offsets
    //! cache.
    //!
    //! These tests exercise the cache primitive directly without needing a
    //! full PDF. End-to-end coverage on a real `/ObjStm`-containing file is
    //! provided indirectly by the existing pdf-syntax tests (the same code
    //! path is now wired through `get_object_stream_offsets_or_init`).

    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    fn make_data() -> Data {
        Data::new(PdfData::from(alloc::vec![0u8; 16]))
    }

    fn id(n: i32) -> ObjectIdentifier {
        ObjectIdentifier::new(n, 0)
    }

    #[test]
    fn qf2b_cache_miss_parses_once_and_returns_same_arc() {
        let d = make_data();
        let calls = AtomicUsize::new(0);

        let a = d
            .get_object_stream_offsets_or_init(id(7), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Some(alloc::vec![(1, 10), (2, 20), (3, 30)])
            })
            .expect("first parse must succeed");

        // Same key, different closure body — should not be invoked.
        let b = d
            .get_object_stream_offsets_or_init(id(7), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Some(alloc::vec![(99, 99)])
            })
            .expect("cache hit must succeed");

        assert_eq!(calls.load(Ordering::SeqCst), 1, "parse called exactly once");
        assert!(Arc::ptr_eq(&a, &b), "cache returns identical Arc");
        assert_eq!(&*a, &alloc::vec![(1u32, 10usize), (2, 20), (3, 30)]);
        assert_eq!(d.object_stream_offsets_cache_len(), 1);
    }

    #[test]
    fn qf2b_cache_miss_with_none_does_not_poison() {
        let d = make_data();
        let first = d.get_object_stream_offsets_or_init(id(9), || None);
        assert!(first.is_none(), "first parse may legitimately fail");
        assert_eq!(
            d.object_stream_offsets_cache_len(),
            0,
            "failed parses must not pollute the cache"
        );

        // Retry must be allowed.
        let retry = d
            .get_object_stream_offsets_or_init(id(9), || Some(alloc::vec![(5, 50)]))
            .expect("retry after None must succeed");
        assert_eq!(&*retry, &alloc::vec![(5u32, 50usize)]);
        assert_eq!(d.object_stream_offsets_cache_len(), 1);
    }

    #[test]
    fn qf2b_distinct_ids_are_isolated() {
        let d = make_data();

        let a = d
            .get_object_stream_offsets_or_init(id(1), || Some(alloc::vec![(1, 10)]))
            .unwrap();
        let b = d
            .get_object_stream_offsets_or_init(id(2), || Some(alloc::vec![(2, 20)]))
            .unwrap();

        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(&*a, &alloc::vec![(1u32, 10usize)]);
        assert_eq!(&*b, &alloc::vec![(2u32, 20usize)]);
        assert_eq!(d.object_stream_offsets_cache_len(), 2);
    }

    #[test]
    fn qf2b_distinct_data_instances_do_not_share_cache() {
        // Security boundary: cache is per-`Data`, never shared across
        // documents.
        let d1 = make_data();
        let d2 = make_data();

        let _ = d1
            .get_object_stream_offsets_or_init(id(7), || Some(alloc::vec![(1, 10)]))
            .unwrap();

        // d2 must miss for the same id.
        let calls = AtomicUsize::new(0);
        let _ = d2
            .get_object_stream_offsets_or_init(id(7), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Some(alloc::vec![(2, 20)])
            })
            .unwrap();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "d2 must invoke its own parse — no cross-document cache"
        );

        let again = d2
            .get_object_stream_offsets_or_init(id(7), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Some(alloc::vec![(3, 30)])
            })
            .unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second lookup on d2 must hit the d2 cache"
        );
        assert_eq!(&*again, &alloc::vec![(2u32, 20usize)]);
    }

    #[test]
    fn qf2b_cache_drops_with_data() {
        // The cache holds `Arc<Vec<...>>`; when the `Data` is dropped the
        // inner allocation must also be reclaimed (no leak into a static
        // map). We assert this by checking strong_count after drop.
        let arc_after_drop = {
            let d = make_data();
            let inner = d
                .get_object_stream_offsets_or_init(id(1), || Some(alloc::vec![(1, 10)]))
                .unwrap();
            assert!(Arc::strong_count(&inner) >= 2, "cache + caller refs");
            inner
        };
        // After dropping `d`, only the caller-held Arc remains.
        assert_eq!(Arc::strong_count(&arc_after_drop), 1);
    }
}
