//! Bounded cache of glyph outlines, shared across one document render.
//!
//! Building a glyph outline parses font data and converts its curves, so the
//! renderer memoizes each distinct outline (the path in glyph space) per
//! (font identity, glyph id) and reuses it for repeated glyphs by
//! transforming the cached path instead of rebuilding it. Only the outline is
//! cached — never rasterized coverage, which would change anti-aliasing at
//! sub-pixel offsets.
//!
//! The cache is an ordinary caller-owned value: no globals, no thread-locals.
//! Rendering stays pure per page — a hit returns the same path the build
//! would have produced, so output is byte-identical with the cache on, off,
//! or freshly cleared. Reusing one cache across the pages of a document (see
//! [`render_with_glyph_cache`](crate::render_with_glyph_cache)) additionally
//! shares outlines across pages.

use kurbo::BezPath;
use std::collections::{HashMap, VecDeque};

/// A cached outline plus its recency stamp.
///
/// The stamp matches the entry's latest `(key, stamp)` pair in the recency
/// queue; older pairs for the key are stale. Stamps restart at 1 when a key
/// is re-inserted after eviction, which is safe: evicting a key consumes
/// every pair it ever pushed (the victim's live pair is the newest pair it
/// pushed, and eviction pops everything up to and including it), so no stale
/// pair of a re-inserted key can linger.
struct Entry {
    path: BezPath,
    touched: u64,
}

/// Bounded least-recently-used cache of glyph outlines.
///
/// Keys are opaque `u128`s combining font identity and glyph id (the
/// renderer passes `identifier().cache_key()`); values are the outlines in
/// glyph space. When the cache is full, inserting a new outline evicts the
/// least-recently-used one. Eviction is always output-neutral — it only
/// decides which outlines get rebuilt.
///
/// A disabled cache ([`set_enabled`](Self::set_enabled)) never hits and
/// stores nothing; every lookup counts as a miss. Already-stored entries are
/// kept while disabled, so re-enabling resumes from them.
pub struct GlyphOutlineCache {
    entries: HashMap<u128, Entry>,
    /// Touch history, oldest-first. Each touch pushes one `(key, stamp)`
    /// pair; a pair is live only if it matches the entry's current stamp.
    /// Stale pairs are skipped on eviction and dropped by compaction, which
    /// keeps touches O(1) amortized instead of the O(capacity) scan an exact
    /// order list would pay per glyph (cf. the image cache in
    /// `pdf_interpret::cache`, whose exact order is cheap only because its
    /// capacity is 32).
    recency: VecDeque<(u128, u64)>,
    /// Reusable slot for outlines that are returned but not retained: builds
    /// made while disabled, or while capacity is zero. Lets the lookup return
    /// `&BezPath` on every path without cloning on hits.
    scratch: Option<BezPath>,
    hits: u64,
    misses: u64,
    capacity: usize,
    enabled: bool,
}

impl std::fmt::Debug for GlyphOutlineCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphOutlineCache")
            .field("len", &self.entries.len())
            .field("hits", &self.hits)
            .field("misses", &self.misses)
            .field("capacity", &self.capacity)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl GlyphOutlineCache {
    /// Default entry bound.
    ///
    /// A typical Latin page uses well under 256 distinct glyphs and a whole
    /// Latin/Cyrillic/Greek document stays in the hundreds, so 4096 serves
    /// common documents with zero evictions. Long CJK documents can reach a
    /// few thousand distinct glyphs overall, but the per-page working set is
    /// in the hundreds, which stays resident. Memory stays modest: a Latin
    /// outline is typically hundreds of bytes to ~2KB and a complex CJK
    /// outline up to ~10KB, so a full cache holds single-digit megabytes —
    /// small next to the page pixmaps (tens of MB).
    pub const DEFAULT_CAPACITY: usize = 4096;

    /// An empty cache with [`DEFAULT_CAPACITY`](Self::DEFAULT_CAPACITY) entries.
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// An empty cache holding at most `capacity` entries.
    ///
    /// A capacity of zero is allowed: lookups always build and nothing is
    /// ever stored (every call counts as a miss).
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            recency: VecDeque::new(),
            scratch: None,
            hits: 0,
            misses: 0,
            capacity,
            enabled: true,
        }
    }

    /// Enable or disable the cache. A disabled cache rebuilds every outline
    /// (each build counts as a miss) and stores nothing; re-enabling resumes
    /// from the entries stored before.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether lookups can hit.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Outlines served from the cache.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Outlines built because the cache was disabled, empty of the key, or
    /// had evicted it. Equal to the number of outline builds through this
    /// cache.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Entries currently stored. Always within [`capacity`](Self::capacity).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no outline is stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Maximum entries stored.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Drop all entries and reset the hit/miss counters.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.scratch = None;
        self.hits = 0;
        self.misses = 0;
    }

    /// Return the outline for `key`, building and storing it on a miss.
    ///
    /// Hits return a reference to the stored path (no clone); builds made
    /// while disabled or at zero capacity are returned through a reusable
    /// slot instead. Callers must not call this again while the returned
    /// reference is alive — the renderer detaches the cache for the duration
    /// of each fill for exactly this reason.
    pub(crate) fn get_or_insert_with(
        &mut self,
        key: u128,
        build: impl FnOnce() -> BezPath,
    ) -> &BezPath {
        if !self.enabled {
            self.misses += 1;
            self.scratch = Some(build());
            return self.scratch.as_ref().expect("outline just stored");
        }
        if self.entries.contains_key(&key) {
            self.hits += 1;
            let touched = {
                let entry = self.entries.get_mut(&key).expect("key just checked");
                entry.touched += 1;
                entry.touched
            };
            self.recency.push_back((key, touched));
            self.compact_recency();
            return &self.entries.get(&key).expect("key just checked").path;
        }
        self.misses += 1;
        let path = build();
        if self.capacity == 0 {
            self.scratch = Some(path);
            return self.scratch.as_ref().expect("outline just stored");
        }
        if self.entries.len() >= self.capacity {
            self.evict_lru();
        }
        self.entries.insert(key, Entry { path, touched: 1 });
        self.recency.push_back((key, 1));
        self.compact_recency();
        &self.entries.get(&key).expect("outline just inserted").path
    }

    /// Remove the least-recently-used entry. Only called on a full (hence
    /// non-empty) cache, so a live victim always exists; the fallback keeps
    /// the bound hard even if that invariant ever broke (any victim is still
    /// output-neutral — it only affects which outline gets rebuilt).
    fn evict_lru(&mut self) {
        while let Some((key, touched)) = self.recency.pop_front() {
            let live = self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.touched == touched);
            if live {
                self.entries.remove(&key);
                return;
            }
        }
        debug_assert!(false, "LRU queue lost its live pair");
        if let Some(victim) = self.entries.keys().next().copied() {
            self.entries.remove(&victim);
        }
    }

    /// Bound the recency queue: every touch pushes one pair, so a long render
    /// of few distinct glyphs would otherwise grow the queue without bound.
    /// The rebuild keeps each entry's single live pair in touch order, so the
    /// front stays the least-recently-used key.
    fn compact_recency(&mut self) {
        let limit = self.capacity.saturating_mul(4).max(16);
        if self.recency.len() <= limit {
            return;
        }
        let live: Vec<(u128, u64)> = self
            .recency
            .drain(..)
            .filter(|(key, touched)| {
                self.entries
                    .get(key)
                    .is_some_and(|entry| entry.touched == *touched)
            })
            .collect();
        self.recency.extend(live);
    }
}

impl Default for GlyphOutlineCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn path_with_seed(seed: f64) -> BezPath {
        let mut path = BezPath::new();
        path.move_to((seed, seed));
        path.line_to((seed + 1.0, seed + 1.0));
        path
    }

    #[test]
    fn hit_returns_cached_path_and_counts_stats() {
        let builds = Cell::new(0u32);
        let mut cache = GlyphOutlineCache::new();
        let first = cache
            .get_or_insert_with(7, || {
                builds.set(builds.get() + 1);
                path_with_seed(1.0)
            })
            .elements()
            .to_vec();
        let second = cache
            .get_or_insert_with(7, || {
                builds.set(builds.get() + 1);
                path_with_seed(2.0)
            })
            .elements()
            .to_vec();
        assert_eq!(first, second, "hit must return the stored path");
        assert_eq!(builds.get(), 1, "one build for two lookups");
        assert_eq!((cache.hits(), cache.misses()), (1, 1));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn evicts_least_recently_used() {
        let builds = Cell::new([0u32; 4]);
        let build = |i: usize| {
            let mut counts = builds.get();
            counts[i] += 1;
            builds.set(counts);
            path_with_seed(i as f64)
        };
        let mut cache = GlyphOutlineCache::with_capacity(2);
        cache.get_or_insert_with(1, || build(1));
        cache.get_or_insert_with(2, || build(2));
        cache.get_or_insert_with(1, || build(1));
        cache.get_or_insert_with(3, || build(3));

        // Key 2 was least-recently-used, so it rebuilds -- and re-fetching
        // three keys through two slots thrashes: each re-fetch evicts the
        // next one asked for, so all three build twice. (FIFO would keep 2
        // and LIFO would keep 1 here, so this still pins LRU order.)
        cache.get_or_insert_with(2, || build(2));
        cache.get_or_insert_with(1, || build(1));
        cache.get_or_insert_with(3, || build(3));
        assert_eq!(builds.get(), [0, 2, 2, 2]);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn bound_holds_for_many_distinct_keys() {
        let mut cache = GlyphOutlineCache::with_capacity(8);
        for key in 0..200u128 {
            cache.get_or_insert_with(key, || path_with_seed(key as f64));
        }
        assert_eq!(cache.len(), 8);
    }

    #[test]
    fn disabled_cache_builds_every_time_and_stores_nothing() {
        let builds = Cell::new(0u32);
        let mut cache = GlyphOutlineCache::new();
        cache.set_enabled(false);
        assert!(!cache.is_enabled());
        for _ in 0..3 {
            cache.get_or_insert_with(1, || {
                builds.set(builds.get() + 1);
                path_with_seed(builds.get() as f64)
            });
        }
        assert_eq!(builds.get(), 3);
        assert_eq!((cache.hits(), cache.misses()), (0, 3));
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        cache.set_enabled(true);
        cache.get_or_insert_with(1, || {
            builds.set(builds.get() + 1);
            path_with_seed(9.0)
        });
        cache.get_or_insert_with(1, || {
            builds.set(builds.get() + 1);
            path_with_seed(10.0)
        });
        assert_eq!(builds.get(), 4, "one build after re-enabling, then a hit");
        assert_eq!((cache.hits(), cache.misses()), (1, 4));
    }

    #[test]
    fn zero_capacity_stores_nothing() {
        let builds = Cell::new(0u32);
        let mut cache = GlyphOutlineCache::with_capacity(0);
        for _ in 0..2 {
            cache.get_or_insert_with(1, || {
                builds.set(builds.get() + 1);
                path_with_seed(1.0)
            });
        }
        assert_eq!(builds.get(), 2);
        assert_eq!(cache.len(), 0);
        assert_eq!((cache.hits(), cache.misses()), (0, 2));
    }

    #[test]
    fn clear_drops_entries_and_resets_stats() {
        let mut cache = GlyphOutlineCache::new();
        cache.get_or_insert_with(1, || path_with_seed(1.0));
        cache.get_or_insert_with(1, || path_with_seed(1.0));
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!((cache.hits(), cache.misses()), (0, 0));
    }

    #[test]
    fn recency_queue_stays_bounded_under_repeated_touches() {
        let mut cache = GlyphOutlineCache::with_capacity(4);
        for key in 0..4u128 {
            cache.get_or_insert_with(key, || path_with_seed(key as f64));
        }
        for _ in 0..10_000 {
            cache.get_or_insert_with(0, || path_with_seed(0.0));
        }
        assert_eq!(cache.len(), 4);
        assert!(
            cache.recency.len() <= 32,
            "queue must compact, len = {}",
            cache.recency.len()
        );
    }

    #[test]
    fn reinserted_key_is_most_recently_used() {
        let mut cache = GlyphOutlineCache::with_capacity(2);
        cache.get_or_insert_with(1, || path_with_seed(1.0));
        cache.get_or_insert_with(2, || path_with_seed(2.0));
        // Evicts 1; re-inserting 1 then evicts 2 (the LRU), not the fresh 1.
        cache.get_or_insert_with(3, || path_with_seed(3.0));
        cache.get_or_insert_with(1, || path_with_seed(1.0));
        cache.get_or_insert_with(4, || path_with_seed(4.0));

        let builds = Cell::new(0u32);
        cache.get_or_insert_with(1, || {
            builds.set(builds.get() + 1);
            path_with_seed(1.0)
        });
        assert_eq!(builds.get(), 0, "re-inserted key 1 must still be stored");
        assert_eq!(cache.len(), 2);
    }
}
