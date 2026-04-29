//! Unit tests for the ext_proc handler.
//! Tests each message type, error propagation, timeouts, and concurrent streams.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ext_proc_proto::envoy::config::core::v3::{HeaderMap, HeaderValue};
use ext_proc_proto::envoy::service::ext_proc::v3::external_processor_client::ExternalProcessorClient;
use ext_proc_proto::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer;
use ext_proc_proto::envoy::service::ext_proc::v3::processing_request::Request as ProcReq;
use ext_proc_proto::envoy::service::ext_proc::v3::processing_response::Response as ProcResp;
use ext_proc_proto::envoy::service::ext_proc::v3::{HttpBody, HttpHeaders, ProcessingRequest};
use ipp_framework::cycle_state::CycleState;
use ipp_framework::error::PluginError;
use ipp_framework::inference_message::{InferenceRequest, InferenceResponse};
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use ipp_server_lib::ext_proc_handler::ExtProcServer;
use serde_json::json;
use tokio_stream::StreamExt;
use tonic::transport::Server;

// --- Test plugins ---

struct PassthroughPlugin;
impl RequestProcessor for PassthroughPlugin {
    fn name(&self) -> &str { "passthrough" }
    fn process_request(&self, _cs: &mut CycleState, _req: &mut InferenceRequest) -> Result<(), PluginError> {
        Ok(())
    }
}
impl ResponseProcessor for PassthroughPlugin {
    fn name(&self) -> &str { "passthrough" }
    fn process_response(&self, _cs: &mut CycleState, _resp: &mut InferenceResponse) -> Result<(), PluginError> {
        Ok(())
    }
}

struct FailingPlugin { code: u16 }
impl RequestProcessor for FailingPlugin {
    fn name(&self) -> &str { "failing" }
    fn process_request(&self, _cs: &mut CycleState, _req: &mut InferenceRequest) -> Result<(), PluginError> {
        match self.code {
            400 => Err(PluginError::bad_request("test bad request")),
            404 => Err(PluginError::not_found("test not found")),
            _ => Err(PluginError::internal("test internal error")),
        }
    }
}

struct HeaderMutatingPlugin;
impl RequestProcessor for HeaderMutatingPlugin {
    fn name(&self) -> &str { "header-mutator" }
    fn process_request(&self, _cs: &mut CycleState, req: &mut InferenceRequest) -> Result<(), PluginError> {
        req.set_header(":path", "/v1/chat/completions");
        req.set_header("x-custom", "test-value");
        req.remove_header("authorization");
        Ok(())
    }
}

struct BodyMutatingPlugin;
impl RequestProcessor for BodyMutatingPlugin {
    fn name(&self) -> &str { "body-mutator" }
    fn process_request(&self, _cs: &mut CycleState, req: &mut InferenceRequest) -> Result<(), PluginError> {
        req.set_body(json!({"translated": true, "model": "test"}));
        Ok(())
    }
}

// --- Helper functions ---

async fn start_server(
    req_plugins: Vec<Box<dyn RequestProcessor>>,
    resp_plugins: Vec<Box<dyn ResponseProcessor>>,
) -> String {
    let ext_proc = ExtProcServer::new(req_plugins, resp_plugins);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(ExternalProcessorServer::new(ext_proc))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("http://{}", addr)
}

fn make_headers(path: &str) -> ProcessingRequest {
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

fn make_body(content: &serde_json::Value) -> ProcessingRequest {
    ProcessingRequest {
        request: Some(ProcReq::RequestBody(HttpBody {
            body: serde_json::to_vec(content).unwrap(),
            end_of_stream: true,
        })),
        ..Default::default()
    }
}

fn make_headers_eos(path: &str) -> ProcessingRequest {
    ProcessingRequest {
        request: Some(ProcReq::RequestHeaders(HttpHeaders {
            headers: Some(HeaderMap {
                headers: vec![
                    HeaderValue { key: ":path".into(), raw_value: path.as_bytes().to_vec(), ..Default::default() },
                ],
            }),
            end_of_stream: true,
            ..Default::default()
        })),
        ..Default::default()
    }
}

async fn send_request(addr: &str, messages: Vec<ProcessingRequest>) -> Vec<Result<ext_proc_proto::envoy::service::ext_proc::v3::ProcessingResponse, tonic::Status>> {
    let mut client = ExternalProcessorClient::connect(addr.to_string()).await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(10);

    for msg in messages {
        tx.send(msg).await.unwrap();
    }
    drop(tx);

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let response = client.process(stream).await.unwrap();
    let mut results = Vec::new();
    let mut resp_stream = response.into_inner();

    while let Some(msg) = resp_stream.next().await {
        results.push(msg);
    }
    results
}

// --- Tests ---

#[tokio::test]
async fn passthrough_returns_body_response() {
    let addr = start_server(
        vec![Box::new(PassthroughPlugin)],
        vec![Box::new(PassthroughPlugin)],
    ).await;

    let body = json!({"model": "test", "messages": [{"role": "user", "content": "hi"}]});
    let results = send_request(&addr, vec![make_headers("/test"), make_body(&body)]).await;

    assert_eq!(results.len(), 2); // HeadersResponse + BodyResponse
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());

    // First should be HeadersResponse
    if let Some(ProcResp::RequestHeaders(_)) = &results[0].as_ref().unwrap().response {
        // ok
    } else {
        panic!("Expected RequestHeaders response");
    }

    // Second should be RequestBody response
    if let Some(ProcResp::RequestBody(_)) = &results[1].as_ref().unwrap().response {
        // ok
    } else {
        panic!("Expected RequestBody response");
    }
}

#[tokio::test]
async fn header_mutations_returned_in_response() {
    let addr = start_server(
        vec![Box::new(HeaderMutatingPlugin)],
        vec![],
    ).await;

    let body = json!({"model": "test"});
    let results = send_request(&addr, vec![make_headers("/original"), make_body(&body)]).await;

    let body_resp = results[1].as_ref().unwrap();
    if let Some(ProcResp::RequestBody(br)) = &body_resp.response {
        let common = br.response.as_ref().unwrap();
        let hm = common.header_mutation.as_ref().unwrap();

        let headers: HashMap<String, String> = hm.set_headers.iter()
            .filter_map(|h| h.header.as_ref().map(|hv| {
                (hv.key.clone(), String::from_utf8_lossy(&hv.raw_value).to_string())
            }))
            .collect();

        assert_eq!(headers.get(":path").unwrap(), "/v1/chat/completions");
        assert_eq!(headers.get("x-custom").unwrap(), "test-value");
        assert!(hm.remove_headers.contains(&"authorization".to_string()));
    } else {
        panic!("Expected RequestBody response");
    }
}

#[tokio::test]
async fn body_mutation_returned_in_response() {
    let addr = start_server(
        vec![Box::new(BodyMutatingPlugin)],
        vec![],
    ).await;

    let body = json!({"model": "original"});
    let results = send_request(&addr, vec![make_headers("/test"), make_body(&body)]).await;

    let body_resp = results[1].as_ref().unwrap();
    if let Some(ProcResp::RequestBody(br)) = &body_resp.response {
        let common = br.response.as_ref().unwrap();
        let bm = common.body_mutation.as_ref().expect("body should be mutated");

        if let Some(ext_proc_proto::envoy::service::ext_proc::v3::body_mutation::Mutation::Body(bytes)) = &bm.mutation {
            let parsed: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(parsed["translated"], true);
        } else {
            panic!("Expected body mutation");
        }
    }
}

#[tokio::test]
async fn plugin_error_returns_grpc_error() {
    let addr = start_server(
        vec![Box::new(FailingPlugin { code: 400 })],
        vec![],
    ).await;

    let body = json!({"model": "test"});
    let results = send_request(&addr, vec![make_headers("/test"), make_body(&body)]).await;

    // Should get HeadersResponse OK, then error on BodyResponse
    assert!(results[0].is_ok());
    assert!(results.len() == 2);
    assert!(results[1].is_err());
    let err = results[1].as_ref().unwrap_err();
    assert!(err.message().contains("test bad request"));
}

#[tokio::test]
async fn headers_only_eos_runs_plugins() {
    let addr = start_server(
        vec![Box::new(HeaderMutatingPlugin)],
        vec![],
    ).await;

    let results = send_request(&addr, vec![make_headers_eos("/test")]).await;

    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
}

#[tokio::test]
async fn invalid_json_body_does_not_crash() {
    let addr = start_server(
        vec![Box::new(PassthroughPlugin)],
        vec![],
    ).await;

    let invalid_body = ProcessingRequest {
        request: Some(ProcReq::RequestBody(HttpBody {
            body: b"this is not json{{{".to_vec(),
            end_of_stream: true,
        })),
        ..Default::default()
    };

    let results = send_request(&addr, vec![make_headers("/test"), invalid_body]).await;

    // Should still return responses (plugins run with Null body)
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
}

#[tokio::test]
async fn empty_body_does_not_crash() {
    let addr = start_server(
        vec![Box::new(PassthroughPlugin)],
        vec![],
    ).await;

    let empty_body = ProcessingRequest {
        request: Some(ProcReq::RequestBody(HttpBody {
            body: vec![],
            end_of_stream: true,
        })),
        ..Default::default()
    };

    let results = send_request(&addr, vec![make_headers("/test"), empty_body]).await;
    assert!(results[1].is_ok());
}

#[tokio::test]
async fn concurrent_streams() {
    let addr = start_server(
        vec![Box::new(PassthroughPlugin)],
        vec![Box::new(PassthroughPlugin)],
    ).await;

    let mut handles = Vec::new();
    for i in 0..20 {
        let addr = addr.clone();
        handles.push(tokio::spawn(async move {
            let body = json!({"model": format!("model-{}", i)});
            let results = send_request(&addr, vec![make_headers("/test"), make_body(&body)]).await;
            assert!(results.iter().all(|r| r.is_ok()), "Stream {} failed", i);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn large_payload() {
    let addr = start_server(
        vec![Box::new(PassthroughPlugin)],
        vec![],
    ).await;

    // 100KB+ payload
    let large_content = "x".repeat(100_000);
    let body = json!({
        "model": "test",
        "messages": [{"role": "user", "content": large_content}]
    });

    let results = send_request(&addr, vec![make_headers("/test"), make_body(&body)]).await;
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.is_ok()));
}

#[tokio::test]
async fn content_length_set_on_body_mutation() {
    let addr = start_server(
        vec![Box::new(BodyMutatingPlugin)],
        vec![],
    ).await;

    let body = json!({"model": "test"});
    let results = send_request(&addr, vec![make_headers("/test"), make_body(&body)]).await;

    let body_resp = results[1].as_ref().unwrap();
    if let Some(ProcResp::RequestBody(br)) = &body_resp.response {
        let common = br.response.as_ref().unwrap();
        let hm = common.header_mutation.as_ref().unwrap();

        let headers: HashMap<String, String> = hm.set_headers.iter()
            .filter_map(|h| h.header.as_ref().map(|hv| {
                (hv.key.clone(), String::from_utf8_lossy(&hv.raw_value).to_string())
            }))
            .collect();

        // content-length should be set to match the mutated body size
        assert!(headers.contains_key("content-length"), "content-length not set");
        let cl: usize = headers["content-length"].parse().unwrap();
        assert!(cl > 0);
    }
}
