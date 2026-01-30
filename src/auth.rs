//! Authentication and API key management.

use crate::models::{ApiKeyInfo, Permission};
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Prefix for API keys.
const API_KEY_PREFIX: &str = "sk_live_";

/// Internal representation of an API key with hashed value.
#[derive(Debug)]
pub struct StoredApiKey {
    /// Unique key identifier.
    pub key_id: String,
    /// SHA-256 hash of the API key.
    pub key_hash: String,
    /// Human-readable name for the key.
    pub name: String,
    /// Permissions granted to this key.
    pub permissions: Vec<Permission>,
    /// Rate limit in requests per minute.
    pub rate_limit: u32,
    /// Creation timestamp in milliseconds.
    pub created_at: u64,
    /// Last used timestamp in milliseconds.
    pub last_used_at: AtomicU64,
}

impl Clone for StoredApiKey {
    fn clone(&self) -> Self {
        Self {
            key_id: self.key_id.clone(),
            key_hash: self.key_hash.clone(),
            name: self.name.clone(),
            permissions: self.permissions.clone(),
            rate_limit: self.rate_limit,
            created_at: self.created_at,
            last_used_at: AtomicU64::new(self.last_used_at.load(Ordering::Relaxed)),
        }
    }
}

impl StoredApiKey {
    /// Convert to ApiKeyInfo (without the hash).
    pub fn to_info(&self) -> ApiKeyInfo {
        let last_used = self.last_used_at.load(Ordering::Relaxed);
        ApiKeyInfo {
            key_id: self.key_id.clone(),
            name: self.name.clone(),
            permissions: self.permissions.clone(),
            rate_limit: self.rate_limit,
            created_at: self.created_at,
            last_used_at: if last_used > 0 { Some(last_used) } else { None },
        }
    }

    /// Check if this key has the given permission.
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission) || self.permissions.contains(&Permission::Admin)
    }

    /// Update the last used timestamp.
    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_used_at.store(now, Ordering::Relaxed);
    }
}

/// Rate limiter using sliding window algorithm.
#[derive(Debug, Default)]
pub struct RateLimiter {
    /// Request timestamps per key_id.
    windows: DashMap<String, VecDeque<u64>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
        }
    }

    /// Check if a request is allowed for the given key.
    /// Returns true if allowed, false if rate limited.
    pub fn check_and_record(&self, key_id: &str, rate_limit: u32) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let window_start = now.saturating_sub(60_000); // 1 minute window

        let mut entry = self.windows.entry(key_id.to_string()).or_default();
        let window = entry.value_mut();

        // Remove old entries outside the window
        while let Some(&front) = window.front() {
            if front < window_start {
                window.pop_front();
            } else {
                break;
            }
        }

        // Check if under limit
        if window.len() < rate_limit as usize {
            window.push_back(now);
            true
        } else {
            false
        }
    }

    /// Clear rate limit data for a key.
    pub fn clear(&self, key_id: &str) {
        self.windows.remove(key_id);
    }
}

/// Store for API keys.
#[derive(Debug, Default)]
pub struct ApiKeyStore {
    /// Keys indexed by key_id.
    keys_by_id: DashMap<String, StoredApiKey>,
    /// Key hashes mapped to key_id for lookup.
    hash_to_id: DashMap<String, String>,
    /// Rate limiter.
    rate_limiter: RateLimiter,
}

impl ApiKeyStore {
    /// Create a new API key store.
    pub fn new() -> Self {
        Self {
            keys_by_id: DashMap::new(),
            hash_to_id: DashMap::new(),
            rate_limiter: RateLimiter::new(),
        }
    }

    /// Generate a new API key.
    fn generate_key() -> String {
        let random_part = Uuid::new_v4().to_string().replace('-', "");
        format!("{}{}", API_KEY_PREFIX, random_part)
    }

    /// Hash an API key.
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Create a new API key.
    /// Returns the key_id and the raw API key (only returned once).
    pub fn create_key(
        &self,
        name: String,
        permissions: Vec<Permission>,
        rate_limit: u32,
    ) -> (String, String) {
        let key_id = Uuid::new_v4().to_string();
        let raw_key = Self::generate_key();
        let key_hash = Self::hash_key(&raw_key);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let stored_key = StoredApiKey {
            key_id: key_id.clone(),
            key_hash: key_hash.clone(),
            name,
            permissions,
            rate_limit,
            created_at: now,
            last_used_at: AtomicU64::new(0),
        };

        self.keys_by_id.insert(key_id.clone(), stored_key);
        self.hash_to_id.insert(key_hash, key_id.clone());

        (key_id, raw_key)
    }

    /// Get a key by its ID.
    pub fn get_by_id(&self, key_id: &str) -> Option<ApiKeyInfo> {
        self.keys_by_id.get(key_id).map(|k| k.to_info())
    }

    /// Validate an API key and return the stored key info if valid.
    pub fn validate_key(&self, raw_key: &str) -> Option<StoredApiKey> {
        let key_hash = Self::hash_key(raw_key);
        let key_id = self.hash_to_id.get(&key_hash)?;
        let stored_key = self.keys_by_id.get(key_id.value())?;
        stored_key.touch();
        Some(stored_key.clone())
    }

    /// Check rate limit for a key.
    pub fn check_rate_limit(&self, key_id: &str, rate_limit: u32) -> bool {
        self.rate_limiter.check_and_record(key_id, rate_limit)
    }

    /// List all API keys.
    pub fn list_keys(&self) -> Vec<ApiKeyInfo> {
        self.keys_by_id
            .iter()
            .map(|entry| entry.to_info())
            .collect()
    }

    /// Delete an API key.
    pub fn delete_key(&self, key_id: &str) -> bool {
        if let Some((_, stored_key)) = self.keys_by_id.remove(key_id) {
            self.hash_to_id.remove(&stored_key.key_hash);
            self.rate_limiter.clear(key_id);
            true
        } else {
            false
        }
    }

    /// Get the number of keys.
    pub fn len(&self) -> usize {
        self.keys_by_id.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.keys_by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_store_create_and_validate() {
        let store = ApiKeyStore::new();

        let (key_id, raw_key) = store.create_key(
            "Test Key".to_string(),
            vec![Permission::Read, Permission::Trade],
            1000,
        );

        assert!(!key_id.is_empty());
        assert!(raw_key.starts_with(API_KEY_PREFIX));

        // Validate the key
        let stored = store.validate_key(&raw_key);
        assert!(stored.is_some());
        let stored = stored.unwrap();
        assert_eq!(stored.key_id, key_id);
        assert_eq!(stored.name, "Test Key");
        assert!(stored.has_permission(Permission::Read));
        assert!(stored.has_permission(Permission::Trade));
        assert!(!stored.has_permission(Permission::Admin));
    }

    #[test]
    fn test_api_key_store_invalid_key() {
        let store = ApiKeyStore::new();

        let result = store.validate_key("sk_live_invalid_key");
        assert!(result.is_none());
    }

    #[test]
    fn test_api_key_store_list_keys() {
        let store = ApiKeyStore::new();

        store.create_key("Key 1".to_string(), vec![Permission::Read], 1000);
        store.create_key("Key 2".to_string(), vec![Permission::Trade], 500);

        let keys = store.list_keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_api_key_store_delete_key() {
        let store = ApiKeyStore::new();

        let (key_id, raw_key) =
            store.create_key("To Delete".to_string(), vec![Permission::Read], 1000);

        assert!(store.validate_key(&raw_key).is_some());
        assert!(store.delete_key(&key_id));
        assert!(store.validate_key(&raw_key).is_none());
        assert!(!store.delete_key(&key_id)); // Already deleted
    }

    #[test]
    fn test_admin_permission_grants_all() {
        let store = ApiKeyStore::new();

        let (_, raw_key) = store.create_key("Admin Key".to_string(), vec![Permission::Admin], 1000);

        let stored = store.validate_key(&raw_key).unwrap();
        assert!(stored.has_permission(Permission::Read));
        assert!(stored.has_permission(Permission::Trade));
        assert!(stored.has_permission(Permission::Admin));
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new();

        // Should allow up to rate_limit requests
        for _ in 0..10 {
            assert!(limiter.check_and_record("test_key", 10));
        }

        // 11th request should be denied
        assert!(!limiter.check_and_record("test_key", 10));
    }

    #[test]
    fn test_rate_limiter_different_keys() {
        let limiter = RateLimiter::new();

        // Fill up key1
        for _ in 0..5 {
            assert!(limiter.check_and_record("key1", 5));
        }
        assert!(!limiter.check_and_record("key1", 5));

        // key2 should still have capacity
        assert!(limiter.check_and_record("key2", 5));
    }

    #[test]
    fn test_stored_api_key_touch() {
        let stored = StoredApiKey {
            key_id: "test".to_string(),
            key_hash: "hash".to_string(),
            name: "Test".to_string(),
            permissions: vec![Permission::Read],
            rate_limit: 1000,
            created_at: 0,
            last_used_at: AtomicU64::new(0),
        };

        assert_eq!(stored.last_used_at.load(Ordering::Relaxed), 0);
        stored.touch();
        assert!(stored.last_used_at.load(Ordering::Relaxed) > 0);
    }
}
