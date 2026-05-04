use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

/// Thread-safe credential store shared between the K8s reconciler (write path)
/// and ext_proc request handlers (read path).
///
/// Concurrency model:
/// - Uses `parking_lot::RwLock` for fast concurrent reads with exclusive writes
/// - Reconciler calls `add_or_update` / `delete` (acquires write lock)
/// - Request handlers call `get` (acquires read lock — non-blocking when no write)
/// - All updates are atomic per-key (entire credential map replaced, not field-by-field)
/// - No credential values are logged at any level
#[derive(Clone)]
pub struct SecretStore {
    inner: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
}

impl SecretStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_or_update(
        &self,
        key: &str,
        credentials: HashMap<String, String>,
    ) -> Result<(), String> {
        if credentials.is_empty() {
            return Err(format!("secret '{}' has no data fields", key));
        }

        for (field, value) in &credentials {
            if value.is_empty() {
                return Err(format!("secret '{}' has empty field '{}'", key, field));
            }
        }

        self.inner.write().insert(key.to_string(), credentials);
        Ok(())
    }

    pub fn delete(&self, key: &str) {
        self.inner.write().remove(key);
    }

    pub fn get(&self, key: &str) -> Option<HashMap<String, String>> {
        self.inner.read().get(key).cloned()
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get() {
        let store = SecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("api-key".to_string(), "sk-123".to_string());
        store.add_or_update("ns/secret1", creds).unwrap();

        let result = store.get("ns/secret1").unwrap();
        assert_eq!(result["api-key"], "sk-123");
    }

    #[test]
    fn empty_credentials_rejected() {
        let store = SecretStore::new();
        let err = store.add_or_update("ns/empty", HashMap::new()).unwrap_err();
        assert!(err.contains("no data fields"));
    }

    #[test]
    fn empty_field_value_rejected() {
        let store = SecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("api-key".to_string(), String::new());
        let err = store.add_or_update("ns/bad", creds).unwrap_err();
        assert!(err.contains("empty field"));
    }

    #[test]
    fn delete_removes() {
        let store = SecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("api-key".to_string(), "sk-123".to_string());
        store.add_or_update("ns/s", creds).unwrap();
        store.delete("ns/s");
        assert!(store.get("ns/s").is_none());
    }
}

#[cfg(test)]
mod deletion_tests {
    use super::*;

    #[test]
    fn delete_removes_credentials() {
        let store = SecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("api-key".into(), "secret-value".into());
        store.add_or_update("ns/my-secret", creds).unwrap();

        assert!(store.get("ns/my-secret").is_some());
        store.delete("ns/my-secret");
        assert!(store.get("ns/my-secret").is_none(), "Secret should be removed after delete");
    }

    #[test]
    fn delete_nonexistent_key_does_not_panic() {
        let store = SecretStore::new();
        store.delete("ns/nonexistent"); // should not panic
    }

    #[test]
    fn validation_failure_does_not_delete_existing() {
        let store = SecretStore::new();
        let mut good_creds = HashMap::new();
        good_creds.insert("api-key".into(), "valid-key".into());
        store.add_or_update("ns/s", good_creds).unwrap();

        // Try to update with empty field — should fail but NOT delete
        let mut bad_creds = HashMap::new();
        bad_creds.insert("api-key".into(), String::new());
        assert!(store.add_or_update("ns/s", bad_creds).is_err());

        // Original should still be there
        let result = store.get("ns/s");
        assert!(result.is_some(), "Validation failure should not delete existing secret");
        assert_eq!(result.unwrap()["api-key"], "valid-key");
    }

    #[test]
    fn empty_credentials_does_not_delete_existing() {
        let store = SecretStore::new();
        let mut good_creds = HashMap::new();
        good_creds.insert("api-key".into(), "valid-key".into());
        store.add_or_update("ns/s", good_creds).unwrap();

        // Try to update with empty map — should fail but NOT delete
        assert!(store.add_or_update("ns/s", HashMap::new()).is_err());

        // Original should still be there
        assert!(store.get("ns/s").is_some(), "Empty credentials should not delete existing secret");
    }

    #[test]
    fn add_after_delete_works() {
        let store = SecretStore::new();
        let mut creds = HashMap::new();
        creds.insert("api-key".into(), "v1".into());
        store.add_or_update("ns/s", creds).unwrap();
        store.delete("ns/s");

        let mut creds2 = HashMap::new();
        creds2.insert("api-key".into(), "v2".into());
        store.add_or_update("ns/s", creds2).unwrap();

        let result = store.get("ns/s").unwrap();
        assert_eq!(result["api-key"], "v2");
    }
}
