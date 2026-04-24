use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

#[derive(Debug, Clone)]
pub struct ExternalModelInfo {
    pub provider: String,
    pub target_model: String,
    pub secret_name: String,
    pub secret_namespace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamespacedName {
    pub namespace: String,
    pub name: String,
}

impl NamespacedName {
    pub fn new(namespace: &str, name: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
        }
    }

    fn key(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

impl std::fmt::Display for NamespacedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.namespace, self.name)
    }
}

#[derive(Clone)]
pub struct ModelInfoStore {
    inner: Arc<RwLock<HashMap<String, ExternalModelInfo>>>,
}

impl ModelInfoStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_or_update(&self, key: &NamespacedName, info: ExternalModelInfo) {
        self.inner.write().insert(key.key(), info);
    }

    pub fn delete(&self, key: &NamespacedName) {
        self.inner.write().remove(&key.key());
    }

    pub fn get(&self, key: &NamespacedName) -> Option<ExternalModelInfo> {
        self.inner.read().get(&key.key()).cloned()
    }
}

impl Default for ModelInfoStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get() {
        let store = ModelInfoStore::new();
        let key = NamespacedName::new("ns", "model1");
        store.add_or_update(
            &key,
            ExternalModelInfo {
                provider: "openai".to_string(),
                target_model: "gpt-4o".to_string(),
                secret_name: "my-secret".to_string(),
                secret_namespace: "ns".to_string(),
            },
        );

        let info = store.get(&key).unwrap();
        assert_eq!(info.provider, "openai");
        assert_eq!(info.target_model, "gpt-4o");
    }

    #[test]
    fn delete_removes_entry() {
        let store = ModelInfoStore::new();
        let key = NamespacedName::new("ns", "model1");
        store.add_or_update(
            &key,
            ExternalModelInfo {
                provider: "openai".to_string(),
                target_model: "gpt-4o".to_string(),
                secret_name: "s".to_string(),
                secret_namespace: "ns".to_string(),
            },
        );
        store.delete(&key);
        assert!(store.get(&key).is_none());
    }

    #[test]
    fn get_missing_returns_none() {
        let store = ModelInfoStore::new();
        assert!(store.get(&NamespacedName::new("ns", "missing")).is_none());
    }

    #[test]
    fn update_overwrites() {
        let store = ModelInfoStore::new();
        let key = NamespacedName::new("ns", "model1");
        store.add_or_update(
            &key,
            ExternalModelInfo {
                provider: "openai".to_string(),
                target_model: "old".to_string(),
                secret_name: "s".to_string(),
                secret_namespace: "ns".to_string(),
            },
        );
        store.add_or_update(
            &key,
            ExternalModelInfo {
                provider: "anthropic".to_string(),
                target_model: "new".to_string(),
                secret_name: "s2".to_string(),
                secret_namespace: "ns".to_string(),
            },
        );
        let info = store.get(&key).unwrap();
        assert_eq!(info.provider, "anthropic");
        assert_eq!(info.target_model, "new");
    }
}
