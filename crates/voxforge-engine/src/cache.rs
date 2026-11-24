use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use sha2::{Digest, Sha256};
use voxforge_core::models::AudioChunk;

/// Entry in the LRU cache: stores the audio chunk and its generation counter for ordering.
struct CacheEntry {
    chunk: AudioChunk,
    gen: u64,
}

/// Thread-safe LRU audio synthesis cache.
///
/// # Design
/// Uses a single `Mutex<CacheInner>` (not two separate `RwLock`s) to avoid TOCTOU windows
/// between the entry map and the LRU order. Eviction is O(1) amortised: a monotonic
/// generation counter (`next_gen`) is incremented on every access/insert; eviction scans
/// only the `HashMap` once to find the minimum-gen entry, which is O(capacity) but only
/// runs when the cache is full — far less often than individual gets.
///
/// Cache hits clone only the `AudioChunk` after releasing no extra lock — the previous
/// design held a read-lock while acquiring a separate write-lock for the LRU deque,
/// creating an unnecessary upgrade pattern.
#[derive(Clone)]
pub struct AudioCache {
    capacity: usize,
    inner: Arc<Mutex<CacheInner>>,
    hits: Arc<AtomicU64>,
    misses: Arc<AtomicU64>,
}

struct CacheInner {
    entries: HashMap<String, CacheEntry>,
    next_gen: u64,
}

impl AudioCache {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            inner: Arc::new(Mutex::new(CacheInner {
                entries: HashMap::with_capacity(cap.min(512)),
                next_gen: 0,
            })),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn compute_key(
        engine_id: &str,
        voice_id: &str,
        speed: f32,
        pitch: f32,
        text: &str,
    ) -> String {
        Self::compute_key_with_profile(engine_id, voice_id, speed, pitch, text, None)
    }

    /// Computes a cache key that optionally binds to a cloned voice profile version.
    /// Passing `profile_version` prevents stale hits after `refine_voice_embedding` bumps the version.
    pub fn compute_key_with_profile(
        engine_id: &str,
        voice_id: &str,
        speed: f32,
        pitch: f32,
        text: &str,
        profile_version: Option<u32>,
    ) -> String {
        Self::compute_key_full(
            engine_id,
            voice_id,
            speed,
            pitch,
            text,
            profile_version,
            None,
            None,
            None,
            None,
        )
    }

    /// Computes a full cache key incorporating optional language override,
    /// style instruction, diffusion step count, and deterministic seed.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_key_full(
        engine_id: &str,
        voice_id: &str,
        speed: f32,
        pitch: f32,
        text: &str,
        profile_version: Option<u32>,
        language: Option<&str>,
        instruct: Option<&str>,
        num_step: Option<u32>,
        seed: Option<u64>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(engine_id.as_bytes());
        hasher.update(b":");
        hasher.update(voice_id.as_bytes());
        hasher.update(b":");
        hasher.update(speed.to_bits().to_be_bytes());
        hasher.update(b":");
        hasher.update(pitch.to_bits().to_be_bytes());
        hasher.update(b":");
        hasher.update(text.as_bytes());
        if let Some(ver) = profile_version {
            hasher.update(b":v");
            hasher.update(ver.to_be_bytes());
        }
        if let Some(lang) = language {
            hasher.update(b":lang=");
            hasher.update(lang.as_bytes());
        }
        if let Some(inst) = instruct {
            hasher.update(b":inst=");
            hasher.update(inst.as_bytes());
        }
        if let Some(steps) = num_step {
            hasher.update(b":steps=");
            hasher.update(steps.to_be_bytes());
        }
        if let Some(s) = seed {
            hasher.update(b":seed=");
            hasher.update(s.to_be_bytes());
        }
        hex::encode(hasher.finalize())
    }

    pub async fn get(
        &self,
        engine_id: &str,
        voice_id: &str,
        speed: f32,
        pitch: f32,
        text: &str,
    ) -> Option<AudioChunk> {
        let key = Self::compute_key(engine_id, voice_id, speed, pitch, text);
        self.get_by_key(&key).await
    }

    pub async fn get_by_key(&self, key: &str) -> Option<AudioChunk> {
        let mut guard = self.inner.lock().await;
        let CacheInner { entries, next_gen } = &mut *guard;

        if let Some(entry) = entries.get_mut(key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            let gen = *next_gen;
            *next_gen = gen + 1;
            entry.gen = gen;
            Some(entry.chunk.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    pub async fn insert(
        &self,
        engine_id: &str,
        voice_id: &str,
        speed: f32,
        pitch: f32,
        text: &str,
        chunk: AudioChunk,
    ) {
        let key = Self::compute_key(engine_id, voice_id, speed, pitch, text);
        self.insert_by_key(key, chunk).await;
    }

    pub async fn insert_by_key(&self, key: String, chunk: AudioChunk) {
        let mut guard = self.inner.lock().await;
        let CacheInner { entries, next_gen } = &mut *guard;

        if let Some(entry) = entries.get_mut(&key) {
            let gen = *next_gen;
            *next_gen = gen + 1;
            entry.chunk = chunk;
            entry.gen = gen;
            return;
        }

        if entries.len() >= self.capacity {
            if let Some(lru_key) = entries
                .iter()
                .min_by_key(|(_, e)| e.gen)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&lru_key);
            }
        }

        let gen = *next_gen;
        *next_gen = gen + 1;
        entries.insert(key, CacheEntry { chunk, gen });
    }

    pub fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }

    pub async fn clear(&self) {
        let mut guard = self.inner.lock().await;
        guard.entries.clear();
        guard.next_gen = 0;
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.entries.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.entries.is_empty()
    }
}

impl Default for AudioCache {
    fn default() -> Self {
        Self::new(500)
    }
}
