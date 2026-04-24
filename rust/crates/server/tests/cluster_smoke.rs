//! Smoke test against cluster-deployed Go and Rust ext_proc servers.
//! Requires port-forwards:
//!   oc port-forward svc/config-a-go -n ipp-benchmark 19004:9004
//!   oc port-forward svc/config-c-rust -n ipp-benchmark 19005:9004
//!
//! Run: cargo test -p ipp-server --test cluster_smoke

use ext_proc_proto::envoy::config::core::v3::{HeaderMap, HeaderValue};
use ext_proc_proto::envoy::service::ext_proc::v3::external_processor_client::ExternalProcessorClient;
use ext_proc_proto::envoy::service::ext_proc::v3::processing_request::Request as ProcReq;
use ext_proc_proto::envoy::service::ext_proc::v3::{HttpBody, HttpHeaders, ProcessingRequest};
use serde_json::json;
use tokio_stream::StreamExt;

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

fn make_body(model: &str) -> ProcessingRequest {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": format!("hello from {model}")}]
    });
    ProcessingRequest {
        request: Some(ProcReq::RequestBody(HttpBody {
            body: serde_json::to_vec(&body).unwrap(),
            end_of_stream: true,
        })),
        ..Default::default()
    }
}

async fn test_server(addr: &str, name: &str, model: &str, path: &str) {
    let mut client = ExternalProcessorClient::connect(format!("http://{}", addr))
        .await
        .unwrap_or_else(|e| panic!("{}: connect failed: {}", name, e));

    let (tx, rx) = tokio::sync::mpsc::channel(10);
    tx.send(make_headers(path)).await.unwrap();
    tx.send(make_body(model)).await.unwrap();
    drop(tx);

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let response = client.process(stream).await
        .unwrap_or_else(|e| panic!("{}: process failed: {}", name, e));

    let mut resp_stream = response.into_inner();
    let mut got_response = false;

    while let Some(msg) = resp_stream.next().await {
        match msg {
            Ok(resp) => {
                println!("{}: got response: {:?}", name, resp.response.as_ref().map(|r| std::mem::discriminant(r)));
                got_response = true;
            }
            Err(e) => panic!("{}: stream error: {}", name, e),
        }
    }

    assert!(got_response, "{}: no response received", name);
    println!("{}: PASS", name);
}

#[tokio::test]
async fn smoke_test_go_server() {
    let addr = std::env::var("IPP_GO_ADDR").unwrap_or("127.0.0.1:19004".into());
    test_server(&addr, "Config A (Go)", "bench-openai", "/ipp-benchmark/bench-openai/v1/chat/completions").await;
}

#[tokio::test]
async fn smoke_test_rust_server() {
    let addr = std::env::var("IPP_RUST_ADDR").unwrap_or("127.0.0.1:19005".into());
    test_server(&addr, "Config C (Rust)", "bench-openai", "/ipp-benchmark/bench-openai/v1/chat/completions").await;
}
