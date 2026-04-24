pub mod model_store;
pub mod reconciler;

use ipp_framework::cycle_state::CycleState;
use ipp_framework::error::PluginError;
use ipp_framework::inference_message::InferenceRequest;
use ipp_framework::plugin::RequestProcessor;
use ipp_framework::state_keys;

use model_store::{ModelInfoStore, NamespacedName};

pub struct ModelProviderResolverPlugin {
    store: ModelInfoStore,
}

impl ModelProviderResolverPlugin {
    pub fn new(store: ModelInfoStore) -> Self {
        Self { store }
    }
}

impl RequestProcessor for ModelProviderResolverPlugin {
    fn name(&self) -> &str {
        "model-provider-resolver"
    }

    fn process_request(
        &self,
        cycle_state: &mut CycleState,
        request: &mut InferenceRequest,
    ) -> Result<(), PluginError> {
        let model = match request.body.get("model").and_then(|v| v.as_str()) {
            Some(m) if !m.is_empty() => m,
            _ => return Ok(()),
        };

        let path = request
            .headers
            .get(":path")
            .cloned()
            .unwrap_or_default();
        let relative_path = sanitize_path(&path);

        let segments: Vec<&str> = relative_path.split('/').collect();
        if segments.len() < 2 || segments[0].is_empty() || segments[1].is_empty() {
            return Ok(());
        }

        let model_key = NamespacedName::new(segments[0], segments[1]);

        let info = match self.store.get(&model_key) {
            Some(info) => info,
            None => return Ok(()),
        };

        if !relative_path.ends_with("chat/completions") {
            return Err(PluginError::bad_request(
                "only /chat/completions input type is supported",
            ));
        }

        if info.target_model != model {
            return Err(PluginError::not_found(format!(
                "model in request body '{}' doesn't match ExternalModel",
                model
            )));
        }

        cycle_state.write(state_keys::PROVIDER, info.provider.clone());
        cycle_state.write(state_keys::MODEL, info.target_model.clone());
        cycle_state.write(state_keys::CREDS_REF_NAME, info.secret_name.clone());
        cycle_state.write(
            state_keys::CREDS_REF_NAMESPACE,
            info.secret_namespace.clone(),
        );

        Ok(())
    }
}

fn sanitize_path(path: &str) -> String {
    let path = path.trim();
    let path = match path.find('?') {
        Some(idx) => &path[..idx],
        None => path,
    };
    path.trim_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use model_store::ExternalModelInfo;
    use serde_json::json;

    fn setup_store() -> ModelInfoStore {
        let store = ModelInfoStore::new();
        store.add_or_update(
            &NamespacedName::new("bbr-e2e", "e2e-openai"),
            ExternalModelInfo {
                provider: "openai".to_string(),
                target_model: "e2e-openai".to_string(),
                secret_name: "e2e-openai".to_string(),
                secret_namespace: "bbr-e2e".to_string(),
            },
        );
        store.add_or_update(
            &NamespacedName::new("bbr-e2e", "e2e-anthropic"),
            ExternalModelInfo {
                provider: "anthropic".to_string(),
                target_model: "e2e-anthropic".to_string(),
                secret_name: "e2e-anthropic".to_string(),
                secret_namespace: "bbr-e2e".to_string(),
            },
        );
        store
    }

    #[test]
    fn resolves_provider_from_path() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"model": "e2e-openai", "messages": []}));
        req.set_header(":path", "/bbr-e2e/e2e-openai/v1/chat/completions");

        plugin.process_request(&mut cs, &mut req).unwrap();

        assert_eq!(cs.read::<String>(state_keys::PROVIDER).unwrap(), "openai");
        assert_eq!(
            cs.read::<String>(state_keys::MODEL).unwrap(),
            "e2e-openai"
        );
        assert_eq!(
            cs.read::<String>(state_keys::CREDS_REF_NAME).unwrap(),
            "e2e-openai"
        );
        assert_eq!(
            cs.read::<String>(state_keys::CREDS_REF_NAMESPACE).unwrap(),
            "bbr-e2e"
        );
    }

    #[test]
    fn resolves_anthropic_provider() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"model": "e2e-anthropic", "messages": []}));
        req.set_header(":path", "/bbr-e2e/e2e-anthropic/v1/chat/completions");

        plugin.process_request(&mut cs, &mut req).unwrap();
        assert_eq!(
            cs.read::<String>(state_keys::PROVIDER).unwrap(),
            "anthropic"
        );
    }

    #[test]
    fn unknown_model_is_passthrough() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"model": "internal-model", "messages": []}));
        req.set_header(":path", "/ns/internal-model/v1/chat/completions");

        plugin.process_request(&mut cs, &mut req).unwrap();
        assert!(cs.try_read::<String>(state_keys::PROVIDER).is_none());
    }

    #[test]
    fn model_mismatch_returns_not_found() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"model": "wrong-model", "messages": []}));
        req.set_header(":path", "/bbr-e2e/e2e-openai/v1/chat/completions");

        let err = plugin.process_request(&mut cs, &mut req).unwrap_err();
        assert_eq!(err.http_status_code(), 404);
    }

    #[test]
    fn missing_model_field_is_passthrough() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"messages": []}));

        plugin.process_request(&mut cs, &mut req).unwrap();
        assert!(cs.try_read::<String>(state_keys::PROVIDER).is_none());
    }

    #[test]
    fn non_chat_completions_path_rejected() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"model": "e2e-openai", "messages": []}));
        req.set_header(":path", "/bbr-e2e/e2e-openai/v1/embeddings");

        let err = plugin.process_request(&mut cs, &mut req).unwrap_err();
        assert_eq!(err.http_status_code(), 400);
    }

    #[test]
    fn path_with_query_params_handled() {
        let plugin = ModelProviderResolverPlugin::new(setup_store());
        let mut cs = CycleState::new();
        let mut req = InferenceRequest::new();
        req.set_body(json!({"model": "e2e-openai", "messages": []}));
        req.set_header(
            ":path",
            "/bbr-e2e/e2e-openai/v1/chat/completions?api-version=2024-02-01",
        );

        plugin.process_request(&mut cs, &mut req).unwrap();
        assert_eq!(cs.read::<String>(state_keys::PROVIDER).unwrap(), "openai");
    }
}
