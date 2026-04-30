use futures::StreamExt;
use kube::api::{Api, DynamicObject};
use kube::runtime::watcher;
use kube::runtime::WatchStreamExt;
use kube::Client;
use tracing::{info, warn};

use super::model_store::{ExternalModelInfo, ModelInfoStore, NamespacedName};

const EXTERNAL_MODEL_GROUP: &str = "maas.opendatahub.io";
const EXTERNAL_MODEL_VERSION: &str = "v1alpha1";
const EXTERNAL_MODEL_KIND: &str = "ExternalModel";
const EXTERNAL_MODEL_PLURAL: &str = "externalmodels";

pub async fn run_external_model_watcher(client: Client, store: ModelInfoStore) {
    loop {
        let ar = kube::discovery::ApiResource {
            group: EXTERNAL_MODEL_GROUP.to_string(),
            version: EXTERNAL_MODEL_VERSION.to_string(),
            api_version: format!("{}/{}", EXTERNAL_MODEL_GROUP, EXTERNAL_MODEL_VERSION),
            kind: EXTERNAL_MODEL_KIND.to_string(),
            plural: EXTERNAL_MODEL_PLURAL.to_string(),
        };

        let api: Api<DynamicObject> = Api::all_with(client.clone(), &ar);

        info!("Starting ExternalModel watcher for {}/{}", EXTERNAL_MODEL_GROUP, EXTERNAL_MODEL_KIND);

        let mut stream = watcher::watcher(api, watcher::Config::default()).applied_objects().boxed();

        loop {
            match stream.next().await {
            Some(Ok(obj)) => {
                let name = obj.metadata.name.clone().unwrap_or_default();
                let namespace = obj.metadata.namespace.clone().unwrap_or_default();
                let key = NamespacedName::new(&namespace, &name);

                // Check if deleted
                if obj.metadata.deletion_timestamp.is_some() {
                    info!(name = %name, namespace = %namespace, "ExternalModel deleted");
                    store.delete(&key);
                    continue;
                }

                // Extract spec fields from unstructured data
                let spec = obj.data.get("spec").and_then(|v| v.as_object());
                if let Some(spec) = spec {
                    let provider = spec
                        .get("provider")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let target_model = spec
                        .get("targetModel")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let secret_name = spec
                        .get("credentialRef")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let info = ExternalModelInfo {
                        provider: provider.clone(),
                        target_model: target_model.clone(),
                        secret_name,
                        secret_namespace: namespace.clone(),
                    };

                    store.add_or_update(&key, info);
                    info!(
                        name = %name,
                        namespace = %namespace,
                        provider = %provider,
                        target_model = %target_model,
                        "Updated model store"
                    );
                }
            }
            Some(Err(e)) => {
                warn!(error = %e, "ExternalModel watcher error");
            }
            None => {
                warn!("ExternalModel watcher stream ended, reconnecting in 5s...");
                break;
            }
        }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
