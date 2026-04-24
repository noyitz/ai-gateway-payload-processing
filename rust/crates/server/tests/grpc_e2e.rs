//! End-to-end gRPC integration tests.
//!
//! These tests start the Rust ext_proc server on a random port,
//! send actual gRPC ext_proc requests, and verify responses.
//! No Kubernetes cluster needed — tests use pre-populated stores.

use std::net::SocketAddr;

use ext_proc_proto::envoy::config::core::v3::{HeaderMap, HeaderValue};
use ext_proc_proto::envoy::service::ext_proc::v3::processing_request::Request as ProcReq;
use ext_proc_proto::envoy::service::ext_proc::v3::{
    external_processor_server::ExternalProcessorServer, HttpBody, HttpHeaders, ProcessingRequest,
};
use ext_proc_proto::envoy::service::ext_proc::v3::external_processor_client::ExternalProcessorClient;
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use ipp_k8s_plugins::apikey_injection::secret_store::SecretStore;
use ipp_k8s_plugins::apikey_injection::ApiKeyInjectionPlugin;
use ipp_k8s_plugins::body_field_to_header::BodyFieldToHeaderPlugin;
use ipp_k8s_plugins::model_provider_resolver::model_store::{
    ExternalModelInfo, ModelInfoStore, NamespacedName,
};
use ipp_k8s_plugins::model_provider_resolver::ModelProviderResolverPlugin;
use ipp_server_lib::ext_proc_handler::ExtProcServer;
use ipp_translators::api_translation_plugin::{ApiTranslationPlugin, VertexOpenAiConfig};
use serde_json::json;
use tokio_stream::StreamExt;
use tonic::transport::Server;

fn setup_stores() -> (ModelInfoStore, SecretStore) {
    let model_store = ModelInfoStore::new();
    let secret_store = SecretStore::new();

    // OpenAI provider
    model_store.add_or_update(
        &NamespacedName::new("bbr-e2e", "e2e-openai"),
        ExternalModelInfo {
            provider: "openai".into(),
            target_model: "e2e-openai".into(),
            secret_name: "e2e-openai".into(),
            secret_namespace: "bbr-e2e".into(),
        },
    );
    let mut creds = std::collections::HashMap::new();
    creds.insert("api-key".into(), "sk-test-openai".into());
    secret_store.add_or_update("bbr-e2e/e2e-openai", creds).unwrap();

    // Anthropic provider
    model_store.add_or_update(
        &NamespacedName::new("bbr-e2e", "e2e-anthropic"),
        ExternalModelInfo {
            provider: "anthropic".into(),
            target_model: "e2e-anthropic".into(),
            secret_name: "e2e-anthropic".into(),
            secret_namespace: "bbr-e2e".into(),
        },
    );
    let mut creds = std::collections::HashMap::new();
    creds.insert("api-key".into(), "sk-test-anthropic".into());
    secret_store.add_or_update("bbr-e2e/e2e-anthropic", creds).unwrap();

    // Azure provider
    model_store.add_or_update(
        &NamespacedName::new("bbr-e2e", "e2e-azure"),
        ExternalModelInfo {
            provider: "azure-openai".into(),
            target_model: "e2e-azure".into(),
            secret_name: "e2e-azure".into(),
            secret_namespace: "bbr-e2e".into(),
        },
    );
    let mut creds = std::collections::HashMap::new();
    creds.insert("api-key".into(), "az-test-key".into());
    secret_store.add_or_update("bbr-e2e/e2e-azure", creds).unwrap();

    // Bedrock provider
    model_store.add_or_update(
        &NamespacedName::new("bbr-e2e", "e2e-bedrock"),
        ExternalModelInfo {
            provider: "bedrock-openai".into(),
            target_model: "e2e-bedrock".into(),
            secret_name: "e2e-bedrock".into(),
            secret_namespace: "bbr-e2e".into(),
        },
    );
    let mut creds = std::collections::HashMap::new();
    creds.insert("api-key".into(), "br-test-key".into());
    secret_store.add_or_update("bbr-e2e/e2e-bedrock", creds).unwrap();

    // Vertex provider
    model_store.add_or_update(
        &NamespacedName::new("bbr-e2e", "e2e-vertex-openai"),
        ExternalModelInfo {
            provider: "vertex-openai".into(),
            target_model: "e2e-vertex-openai".into(),
            secret_name: "e2e-vertex-openai".into(),
            secret_namespace: "bbr-e2e".into(),
        },
    );
    let mut creds = std::collections::HashMap::new();
    creds.insert("api-key".into(), "vx-test-key".into());
    secret_store.add_or_update("bbr-e2e/e2e-vertex-openai", creds).unwrap();

    (model_store, secret_store)
}

async fn start_server() -> SocketAddr {
    let (model_store, secret_store) = setup_stores();

    let body_to_header = BodyFieldToHeaderPlugin::new("model", "X-Gateway-Model-Name").unwrap();
    let model_resolver = ModelProviderResolverPlugin::new(model_store);
    let api_translation = ApiTranslationPlugin::new(Some(VertexOpenAiConfig {
        project: "test-project".into(),
        location: "us-central1".into(),
        endpoint: "openapi".into(),
    }))
    .unwrap();
    let apikey_injection = ApiKeyInjectionPlugin::new(secret_store);

    let api_translation_resp = ApiTranslationPlugin::new(Some(VertexOpenAiConfig {
        project: "test-project".into(),
        location: "us-central1".into(),
        endpoint: "openapi".into(),
    }))
    .unwrap();

    let request_plugins: Vec<Box<dyn RequestProcessor>> = vec![
        Box::new(body_to_header),
        Box::new(model_resolver),
        Box::new(api_translation),
        Box::new(apikey_injection),
    ];
    let response_plugins: Vec<Box<dyn ResponseProcessor>> = vec![Box::new(api_translation_resp)];

    let ext_proc = ExtProcServer::new(request_plugins, response_plugins);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(ExternalProcessorServer::new(ext_proc))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    addr
}

fn make_request_headers(path: &str) -> ProcessingRequest {
    ProcessingRequest {
        request: Some(ProcReq::RequestHeaders(HttpHeaders {
            headers: Some(HeaderMap {
                headers: vec![
                    HeaderValue { key: ":path".into(), raw_value: path.as_bytes().to_vec(), ..Default::default() },
                    HeaderValue { key: ":method".into(), raw_value: b"POST".to_vec(), ..Default::default() },
                    HeaderValue { key: "content-type".into(), raw_value: b"application/json".to_vec(), ..Default::default() },
                ],
            }),
            end_of_stream: false,
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn make_request_body(model: &str) -> ProcessingRequest {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": format!("hello from {}", model)}]
    });
    ProcessingRequest {
        request: Some(ProcReq::RequestBody(HttpBody {
            body: serde_json::to_vec(&body).unwrap(),
            end_of_stream: true,
        })),
        ..Default::default()
    }
}

struct ProviderTest {
    name: &'static str,
    provider: &'static str,
    model: &'static str,
    expected_path: &'static str,
    expected_auth_header: &'static str,
    expected_auth_prefix: &'static str,
}

const PROVIDER_TESTS: &[ProviderTest] = &[
    ProviderTest {
        name: "openai",
        provider: "openai",
        model: "e2e-openai",
        expected_path: "/v1/chat/completions",
        expected_auth_header: "Authorization",
        expected_auth_prefix: "Bearer ",
    },
    ProviderTest {
        name: "anthropic",
        provider: "anthropic",
        model: "e2e-anthropic",
        expected_path: "/v1/messages",
        expected_auth_header: "x-api-key",
        expected_auth_prefix: "",
    },
    ProviderTest {
        name: "azure-openai",
        provider: "azure-openai",
        model: "e2e-azure",
        expected_path: "/openai/v1/chat/completions",
        expected_auth_header: "api-key",
        expected_auth_prefix: "",
    },
    ProviderTest {
        name: "bedrock-openai",
        provider: "bedrock-openai",
        model: "e2e-bedrock",
        expected_path: "/v1/chat/completions",
        expected_auth_header: "Authorization",
        expected_auth_prefix: "Bearer ",
    },
    ProviderTest {
        name: "vertex-openai",
        provider: "vertex-openai",
        model: "e2e-vertex-openai",
        expected_path: "/v1/projects/test-project/locations/us-central1/endpoints/openapi/chat/completions",
        expected_auth_header: "Authorization",
        expected_auth_prefix: "Bearer ",
    },
];

#[tokio::test]
async fn all_providers_process_request_through_grpc() {
    let addr = start_server().await;

    for tc in PROVIDER_TESTS {
        let mut client = ExternalProcessorClient::connect(format!("http://{}", addr))
            .await
            .unwrap_or_else(|e| panic!("Failed to connect for {}: {}", tc.name, e));

        let path = format!("/bbr-e2e/{}/v1/chat/completions", tc.model);
        let (tx, rx) = tokio::sync::mpsc::channel(10);

        tx.send(make_request_headers(&path)).await.unwrap();
        tx.send(make_request_body(tc.model)).await.unwrap();
        drop(tx);

        let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let response = client.process(request_stream).await;

        match response {
            Ok(resp) => {
                let mut stream = resp.into_inner();
                let mut got_body_response = false;

                while let Some(msg) = stream.next().await {
                    match msg {
                        Ok(proc_resp) => {
                            if let Some(ext_proc_proto::envoy::service::ext_proc::v3::processing_response::Response::RequestBody(body_resp)) = proc_resp.response {
                                got_body_response = true;
                                let common = body_resp.response.unwrap();

                                // Verify headers were mutated
                                if let Some(hm) = &common.header_mutation {
                                    let headers: std::collections::HashMap<String, String> = hm
                                        .set_headers
                                        .iter()
                                        .filter_map(|h| {
                                            h.header.as_ref().map(|hv| {
                                                let val = if !hv.raw_value.is_empty() {
                                                    String::from_utf8_lossy(&hv.raw_value).to_string()
                                                } else {
                                                    hv.value.clone()
                                                };
                                                (hv.key.clone(), val)
                                            })
                                        })
                                        .collect();

                                    // Check path rewrite
                                    assert_eq!(
                                        headers.get(":path").map(String::as_str),
                                        Some(tc.expected_path),
                                        "{}: wrong :path header",
                                        tc.name
                                    );

                                    // Check auth header injected
                                    assert!(
                                        headers.contains_key(tc.expected_auth_header),
                                        "{}: missing {} header",
                                        tc.name,
                                        tc.expected_auth_header
                                    );

                                    let auth_value = headers.get(tc.expected_auth_header).unwrap();
                                    assert!(
                                        auth_value.starts_with(tc.expected_auth_prefix),
                                        "{}: auth header '{}' should start with '{}'",
                                        tc.name,
                                        auth_value,
                                        tc.expected_auth_prefix
                                    );

                                    // Check authorization removed
                                    assert!(
                                        hm.remove_headers.contains(&"authorization".to_string()),
                                        "{}: authorization header should be removed",
                                        tc.name
                                    );
                                }

                                // For Anthropic, verify body was mutated
                                if tc.provider == "anthropic" {
                                    assert!(
                                        common.body_mutation.is_some(),
                                        "anthropic: body should be mutated"
                                    );
                                    if let Some(bm) = &common.body_mutation {
                                        if let Some(ext_proc_proto::envoy::service::ext_proc::v3::body_mutation::Mutation::Body(body_bytes)) = &bm.mutation {
                                            let body: serde_json::Value = serde_json::from_slice(body_bytes).unwrap();
                                            assert!(body.get("max_tokens").is_some(), "anthropic: should have max_tokens");
                                            assert!(body.get("messages").is_some(), "anthropic: should have messages");
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => panic!("{}: gRPC error: {}", tc.name, e),
                    }
                }

                assert!(
                    got_body_response,
                    "{}: did not receive RequestBody response",
                    tc.name
                );
            }
            Err(e) => panic!("{}: process() failed: {}", tc.name, e),
        }
    }
}
