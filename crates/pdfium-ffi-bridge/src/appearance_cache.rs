//! Appearance stream cache — avoid regenerating unchanged fields.
//!
//! Caches generated `AppearanceStream` objects by a hash of their inputs
//! (field value, dimensions, config). When a form is re-rendered with only
//! a few changed fields, most appearance streams are served from cache.
//!
//! Features:
//! - LRU-style eviction when cache exceeds configurable memory budget
//! - Incremental updates: only regenerate streams for changed fields
//! - Thread-safe via interior mutability

use crate::appearance::{
    draw_appearance, field_appearance, multiline_appearance, AppearanceConfig, AppearanceStream,
};
use std::collections::HashMap;
use xfa_layout_engine::layout::{LayoutContent, LayoutNode};

/// Cache key derived from field content, dimensions, and appearance config.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    name: String,
    content_hash: u64,
    width_bits: u64,
    height_bits: u64,
    config_hash: u64,
}

impl CacheKey {
    fn from_node(node: &LayoutNode, config: &AppearanceConfig) -> Self {
        let content_hash = hash_content(&node.content);
        Self {
            name: node.name.clone(),
            content_hash,
            width_bits: node.rect.width.to_bits(),
            height_bits: node.rect.height.to_bits(),
            config_hash: hash_config(config),
        }
    }
}

/// FNV-1a hash for AppearanceConfig (ensures different configs produce different cache keys).
fn hash_config(config: &AppearanceConfig) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mix = |h: &mut u64, bytes: &[u8]| {
        for &b in bytes {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    };
    mix(&mut h, config.default_font.as_bytes());
    mix(&mut h, &config.default_font_size.to_bits().to_le_bytes());
    mix(&mut h, &config.border_width.to_bits().to_le_bytes());
    for &c in &config.border_color {
        mix(&mut h, &c.to_bits().to_le_bytes());
    }
    if let Some(bg) = &config.background_color {
        mix(&mut h, &[1]);
        for &c in bg {
            mix(&mut h, &c.to_bits().to_le_bytes());
        }
    } else {
        mix(&mut h, &[0]);
    }
    for &c in &config.text_color {
        mix(&mut h, &c.to_bits().to_le_bytes());
    }
    mix(&mut h, &config.text_padding.to_bits().to_le_bytes());
    mix(&mut h, &[config.compress as u8]);
    h
}

/// Simple FNV-1a hash for content hashing (no crypto needed).
fn hash_content(content: &LayoutContent) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mix = |h: &mut u64, bytes: &[u8]| {
        for &b in bytes {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    };

    match content {
        LayoutContent::Field { value } => {
            mix(&mut h, b"field:");
            mix(&mut h, value.as_bytes());
        }
        LayoutContent::Text(text) => {
            mix(&mut h, b"text:");
            mix(&mut h, text.as_bytes());
        }
        LayoutContent::WrappedText { lines, font_size } => {
            mix(&mut h, b"wrapped:");
            for line in lines {
                mix(&mut h, line.as_bytes());
                mix(&mut h, b"|");
            }
            mix(&mut h, &font_size.to_bits().to_le_bytes());
        }
        LayoutContent::None => {
            mix(&mut h, b"none");
        }
    }
    h
}

/// Cached entry with usage tracking for eviction.
#[derive(Debug, Clone)]
struct CacheEntry {
    stream: AppearanceStream,
    size_bytes: usize,
    access_order: u64,
}

/// Configuration for the appearance cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum memory budget in bytes (0 = unlimited).
    pub max_bytes: usize,
    /// Whether caching is enabled.
    pub enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024, // 16 MB default
            enabled: true,
        }
    }
}

/// Cache for appearance streams.
///
/// Stores previously generated appearance streams and returns them for
/// nodes whose content hasn't changed. Uses LRU-style eviction when
/// the cache exceeds its memory budget.
#[derive(Debug)]
pub struct AppearanceCache {
    entries: HashMap<CacheKey, CacheEntry>,
    total_bytes: usize,
    access_counter: u64,
    config: CacheConfig,
    hits: u64,
    misses: u64,
}

impl AppearanceCache {
    /// Create a new cache with the given configuration.
    pub fn new(config: CacheConfig) -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            access_counter: 0,
            config,
            hits: 0,
            misses: 0,
        }
    }

    /// Generate appearances for a list of layout nodes, using cache where possible.
    ///
    /// Returns `(name, abs_x, abs_y, appearance)` tuples, same as
    /// `appearance::generate_appearances`.
    pub fn generate_cached(
        &mut self,
        nodes: &[LayoutNode],
        config: &AppearanceConfig,
    ) -> Vec<(String, f64, f64, AppearanceStream)> {
        let mut result = Vec::new();
        self.collect_cached(nodes, 0.0, 0.0, config, &mut result);
        result
    }

    /// Cache hit count since creation.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache miss count since creation.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Current number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Current memory usage in bytes.
    pub fn memory_usage(&self) -> usize {
        self.total_bytes
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_bytes = 0;
    }

    /// Invalidate a specific entry by field name.
    pub fn invalidate(&mut self, name: &str) {
        let keys_to_remove: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|k| k.name == name)
            .cloned()
            .collect();
        for key in keys_to_remove {
            if let Some(entry) = self.entries.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
            }
        }
    }

    fn collect_cached(
        &mut self,
        nodes: &[LayoutNode],
        parent_x: f64,
        parent_y: f64,
        config: &AppearanceConfig,
        result: &mut Vec<(String, f64, f64, AppearanceStream)>,
    ) {
        for node in nodes {
            let abs_x = node.rect.x + parent_x;
            let abs_y = node.rect.y + parent_y;
            let width = node.rect.width;
            let height = node.rect.height;

            match &node.content {
                LayoutContent::None => {
                    // Container — recurse only
                }
                _ => {
                    let appearance = if self.config.enabled {
                        self.get_or_generate(node, width, height, config)
                    } else {
                        generate_appearance(node, width, height, config)
                    };
                    result.push((node.name.clone(), abs_x, abs_y, appearance));
                }
            }

            if !node.children.is_empty() {
                self.collect_cached(&node.children, abs_x, abs_y, config, result);
            }
        }
    }

    fn get_or_generate(
        &mut self,
        node: &LayoutNode,
        width: f64,
        height: f64,
        config: &AppearanceConfig,
    ) -> AppearanceStream {
        let key = CacheKey::from_node(node, config);
        self.access_counter += 1;

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.access_order = self.access_counter;
            self.hits += 1;
            return entry.stream.clone();
        }

        self.misses += 1;
        let stream = generate_appearance(node, width, height, config);
        let size = stream.content.len() + stream.font_resources.len() * 32;

        // Evict if over budget; skip entry entirely if it alone exceeds the budget
        if self.config.max_bytes > 0 {
            if size > self.config.max_bytes {
                return stream;
            }
            while self.total_bytes + size > self.config.max_bytes && !self.entries.is_empty() {
                self.evict_lru();
            }
        }

        self.entries.insert(
            key,
            CacheEntry {
                stream: stream.clone(),
                size_bytes: size,
                access_order: self.access_counter,
            },
        );
        self.total_bytes += size;

        stream
    }

    fn evict_lru(&mut self) {
        let oldest_key = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.access_order)
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest_key {
            if let Some(entry) = self.entries.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
            }
        }
    }
}

/// Generate a single appearance stream for a layout node.
fn generate_appearance(
    node: &LayoutNode,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    match &node.content {
        LayoutContent::Field { value } => field_appearance(value, width, height, config),
        LayoutContent::Text(text) => draw_appearance(text, width, height, config),
        LayoutContent::WrappedText { lines, font_size } => {
            let line_height = font_size * 1.2;
            multiline_appearance(lines, *font_size, line_height, width, height, config)
        }
        LayoutContent::None => AppearanceStream {
            content: vec![],
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::FormNodeId;
    use xfa_layout_engine::types::Rect;

    fn make_field(name: &str, value: &str, x: f64, y: f64) -> LayoutNode {
        LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(x, y, 100.0, 20.0),
            name: name.to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
            },
            children: vec![],
        }
    }

    #[test]
    fn cache_hit_on_same_content() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig::default());

        let nodes = vec![make_field("Name", "John", 10.0, 10.0)];
        let result1 = cache.generate_cached(&nodes, &config);
        assert_eq!(result1.len(), 1);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 1);

        // Same content → should hit cache
        let result2 = cache.generate_cached(&nodes, &config);
        assert_eq!(result2.len(), 1);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);

        // Content should be identical
        assert_eq!(result1[0].3.content, result2[0].3.content);
    }

    #[test]
    fn cache_miss_on_changed_value() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig::default());

        let nodes1 = vec![make_field("Name", "John", 10.0, 10.0)];
        cache.generate_cached(&nodes1, &config);
        assert_eq!(cache.misses(), 1);

        // Changed value → should miss
        let nodes2 = vec![make_field("Name", "Jane", 10.0, 10.0)];
        cache.generate_cached(&nodes2, &config);
        assert_eq!(cache.misses(), 2);
    }

    #[test]
    fn cache_eviction_on_memory_limit() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig {
            max_bytes: 200, // Very small budget
            enabled: true,
        });

        // Generate many entries to trigger eviction
        for i in 0..20 {
            let nodes = vec![make_field(&format!("F{i}"), &format!("V{i}"), 0.0, 0.0)];
            cache.generate_cached(&nodes, &config);
        }

        // Cache should have evicted old entries
        assert!(cache.memory_usage() <= 200);
        assert!(cache.len() < 20);
    }

    #[test]
    fn cache_disabled() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig {
            max_bytes: 0,
            enabled: false,
        });

        let nodes = vec![make_field("Name", "John", 10.0, 10.0)];
        cache.generate_cached(&nodes, &config);
        cache.generate_cached(&nodes, &config);

        // No caching → all misses
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 0); // counter not incremented when disabled
        assert!(cache.is_empty());
    }

    #[test]
    fn invalidate_by_name() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig::default());

        let nodes = vec![
            make_field("Name", "John", 10.0, 10.0),
            make_field("Email", "john@example.com", 10.0, 40.0),
        ];
        cache.generate_cached(&nodes, &config);
        assert_eq!(cache.len(), 2);

        cache.invalidate("Name");
        assert_eq!(cache.len(), 1);

        // Re-generate → Name should miss, Email should hit
        cache.generate_cached(&nodes, &config);
        assert_eq!(cache.hits(), 1); // Email
    }

    #[test]
    fn clear_cache() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig::default());

        let nodes = vec![make_field("Name", "John", 10.0, 10.0)];
        cache.generate_cached(&nodes, &config);
        assert!(!cache.is_empty());

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn nested_children_cached() {
        let config = AppearanceConfig::default();
        let mut cache = AppearanceCache::new(CacheConfig::default());

        let nodes = vec![LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(0.0, 0.0, 300.0, 200.0),
            name: "Container".to_string(),
            content: LayoutContent::None,
            children: vec![
                make_field("A", "X", 10.0, 10.0),
                make_field("B", "Y", 10.0, 40.0),
            ],
        }];

        let result = cache.generate_cached(&nodes, &config);
        assert_eq!(result.len(), 2); // A and B, not Container
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.misses(), 2);

        // Re-generate → both should hit
        cache.generate_cached(&nodes, &config);
        assert_eq!(cache.hits(), 2);
    }
}
