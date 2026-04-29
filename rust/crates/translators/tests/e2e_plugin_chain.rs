//! Integration tests replicating the Go E2E test suite.
//!
//! The Go E2E tests (test/e2e/e2e_test.go) validate:
//!   1. "should return 200 for provider X" — request flows through plugin chain without error
//!   2. "should return OpenAI format response" — response has `choices` and `model` keys
//!   3. "should reject requests with invalid API key" — wrong key returns 401
//!
//! These tests replicate scenarios 1 & 2 at the plugin level (no cluster needed).
//! Each test sends an OpenAI-format request through the APITranslationPlugin for a
//! given provider, then feeds a simulated provider response back through the response
//! plugin, and validates the output is OpenAI-format.
//!
//! Test providers match the Go E2E suite:
//!   - e2e-openai (openai)
//!   - e2e-anthropic (anthropic)
//!   - e2e-azure (azure-openai)
//!   - e2e-bedrock (bedrock-openai)
//!   - e2e-vertex-openai (vertex-openai)

use ipp_framework::cycle_state::CycleState;
use ipp_framework::inference_message::{InferenceRequest, InferenceResponse};
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use ipp_framework::state_keys;
use ipp_translators::api_translation_plugin::{ApiTranslationPlugin, VertexOpenAiConfig};
use serde_json::{json, Value};

fn make_plugin() -> ApiTranslationPlugin {
    ApiTranslationPlugin::new(Some(VertexOpenAiConfig {
        project: "test-project".to_string(),
        location: "us-central1".to_string(),
        endpoint: "openapi".to_string(),
    }))
    .unwrap()
}

fn openai_chat_request(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [{"role": "user", "content": format!("hello from {model}")}]
    })
}

/// Simulates a provider response for each provider type.
/// The real simulator (llm-katan) returns these shapes.
fn provider_response(provider: &str) -> Value {
    match provider {
        "anthropic" => json!({
            "id": "msg_sim_123",
            "type": "message",
            "model": "claude-3-5-sonnet-20241022",
            "content": [{"type": "text", "text": "Hello! I'm Claude, simulated response."}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 15, "output_tokens": 12}
        }),
        "azure-openai" => json!({
            "id": "chatcmpl-sim-az",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello from Azure!"},
                "finish_reason": "stop",
                "content_filter_results": {"hate": {"filtered": false, "severity": "safe"}}
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18},
            "prompt_filter_results": [{"prompt_index": 0}]
        }),
        "vertex-openai" => json!({
            "id": "chatcmpl-sim-vx",
            "object": "chat.completion",
            "model": "google/gemini-2.0-flash",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello from Vertex!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18,
                "extra_properties": {"cached_content_token_count": 0}
            }
        }),
        _ => json!({
            "id": "chatcmpl-sim-oai",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello from simulator!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18}
        }),
    }
}

fn validate_openai_response(body: &Value, test_name: &str) {
    assert!(
        body.get("choices").is_some(),
        "{test_name}: response missing 'choices'"
    );
    assert!(
        body.get("model").is_some(),
        "{test_name}: response missing 'model'"
    );

    let choices = body["choices"].as_array().expect("choices should be array");
    assert!(
        !choices.is_empty(),
        "{test_name}: choices should not be empty"
    );

    let first = &choices[0];
    assert!(
        first.get("message").is_some() || first.get("delta").is_some(),
        "{test_name}: choice missing 'message' or 'delta'"
    );
}

struct ProviderTestCase {
    name: &'static str,
    provider: &'static str,
    model: &'static str,
}

const PROVIDERS: &[ProviderTestCase] = &[
    ProviderTestCase {
        name: "e2e-openai",
        provider: "openai",
        model: "e2e-openai",
    },
    ProviderTestCase {
        name: "e2e-anthropic",
        provider: "anthropic",
        model: "e2e-anthropic",
    },
    ProviderTestCase {
        name: "e2e-azure",
        provider: "azure-openai",
        model: "e2e-azure",
    },
    ProviderTestCase {
        name: "e2e-bedrock",
        provider: "bedrock-openai",
        model: "e2e-bedrock",
    },
    ProviderTestCase {
        name: "e2e-vertex-openai",
        provider: "vertex-openai",
        model: "e2e-vertex-openai",
    },
];

/// Replicates: "should return 200 for provider X"
/// Verifies that a request flows through the plugin chain without error.
#[test]
fn request_succeeds_for_all_providers() {
    let plugin = make_plugin();

    for tc in PROVIDERS {
        let mut cs = CycleState::new();
        cs.write(state_keys::PROVIDER, tc.provider.to_string());
        cs.write(state_keys::MODEL, tc.model.to_string());

        let mut req = InferenceRequest::new();
        req.set_body(openai_chat_request(tc.model));

        let result = plugin.process_request(&mut cs, &mut req);
        assert!(
            result.is_ok(),
            "Provider '{}' request failed: {:?}",
            tc.provider,
            result.err()
        );

        // Verify path header was set (all providers rewrite :path)
        assert!(
            req.headers.contains_key(":path"),
            "Provider '{}' did not set :path header",
            tc.provider
        );
    }
}

/// Replicates: "should return OpenAI format response for provider X"
/// Verifies response has `choices` and `model` after translation.
#[test]
fn response_is_openai_format_for_all_providers() {
    let plugin = make_plugin();

    for tc in PROVIDERS {
        let mut cs = CycleState::new();
        cs.write(state_keys::PROVIDER, tc.provider.to_string());
        cs.write(state_keys::MODEL, tc.model.to_string());

        let mut resp = InferenceResponse::new();
        resp.inner.body = provider_response(tc.provider);

        let result = plugin.process_response(&mut cs, &mut resp);
        assert!(
            result.is_ok(),
            "Provider '{}' response failed: {:?}",
            tc.provider,
            result.err()
        );

        validate_openai_response(&resp.body, tc.name);
    }
}

/// Verifies the full round-trip: request translation + response translation.
/// This is the closest to what the E2E tests validate end-to-end.
#[test]
fn full_roundtrip_all_providers() {
    let plugin = make_plugin();

    for tc in PROVIDERS {
        // --- Request phase ---
        let mut cs = CycleState::new();
        cs.write(state_keys::PROVIDER, tc.provider.to_string());
        cs.write(state_keys::MODEL, tc.model.to_string());

        let mut req = InferenceRequest::new();
        req.set_body(openai_chat_request(tc.model));
        req.set_header("authorization", "Bearer user-token");

        plugin
            .process_request(&mut cs, &mut req)
            .unwrap_or_else(|e| panic!("Request failed for {}: {e}", tc.provider));

        // authorization should be stripped
        assert!(
            !req.headers.contains_key("authorization"),
            "{}: authorization header not removed",
            tc.provider
        );

        // --- Response phase ---
        let mut resp = InferenceResponse::new();
        resp.inner.body = provider_response(tc.provider);

        plugin
            .process_response(&mut cs, &mut resp)
            .unwrap_or_else(|e| panic!("Response failed for {}: {e}", tc.provider));

        validate_openai_response(&resp.body, tc.name);

        // Provider-specific response field stripping
        match tc.provider {
            "azure-openai" => {
                assert!(
                    resp.body.get("prompt_filter_results").is_none(),
                    "azure: prompt_filter_results not stripped"
                );
                assert!(
                    resp.body["choices"][0]
                        .get("content_filter_results")
                        .is_none(),
                    "azure: content_filter_results not stripped"
                );
            }
            "vertex-openai" => {
                assert!(
                    resp.body["usage"].get("extra_properties").is_none(),
                    "vertex: extra_properties not stripped"
                );
            }
            _ => {}
        }
    }
}

/// Verifies Anthropic-specific: request body is fully transformed.
#[test]
fn anthropic_request_body_transformed() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "anthropic".to_string());

    let body = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "What is 2+2?"}
        ],
        "max_tokens": 100,
        "temperature": 0.7
    });

    let mut req = InferenceRequest::new();
    req.set_body(body);

    plugin.process_request(&mut cs, &mut req).unwrap();

    // Anthropic-specific checks
    assert_eq!(req.body["system"], "You are helpful.");
    assert_eq!(req.body["max_tokens"], 100);
    assert_eq!(req.body["temperature"], 0.7);
    // Messages should only contain the user message (system extracted)
    let msgs = req.body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
}

/// Verifies Anthropic tool calling round-trip.
#[test]
fn anthropic_tool_calling_roundtrip() {
    let plugin = make_plugin();

    // --- Request with tools ---
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "anthropic".to_string());
    cs.write(
        state_keys::MODEL,
        "claude-3-5-sonnet-20241022".to_string(),
    );

    let body = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [{"role": "user", "content": "What's the weather in NYC?"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather for a location",
                "parameters": {
                    "type": "object",
                    "properties": {"location": {"type": "string"}},
                    "required": ["location"]
                }
            }
        }]
    });

    let mut req = InferenceRequest::new();
    req.set_body(body);
    plugin.process_request(&mut cs, &mut req).unwrap();

    // Verify Anthropic tool format
    let tools = req.body["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "get_weather");
    assert!(tools[0].get("input_schema").is_some());

    // --- Response with tool_use ---
    let mut resp = InferenceResponse::new();
    resp.inner.body = json!({
        "id": "msg_tool_1",
        "type": "message",
        "model": "claude-3-5-sonnet-20241022",
        "content": [
            {"type": "text", "text": "I'll check the weather."},
            {"type": "tool_use", "id": "tu_1", "name": "get_weather", "input": {"location": "NYC"}}
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 20, "output_tokens": 30}
    });

    plugin.process_response(&mut cs, &mut resp).unwrap();

    // Verify OpenAI tool_calls format
    validate_openai_response(&resp.body, "anthropic-tool-calling");
    assert_eq!(resp.body["choices"][0]["finish_reason"], "tool_calls");
    let tool_calls = resp.body["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
}

/// Verifies error handling: missing model field.
#[test]
fn missing_model_returns_error() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "openai".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({"messages": [{"role": "user", "content": "Hi"}]}));

    let err = plugin.process_request(&mut cs, &mut req).unwrap_err();
    assert_eq!(err.http_status_code(), 400);
    assert!(err.msg.contains("model"));
}

/// Verifies error handling: empty messages.
#[test]
fn empty_messages_returns_error() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "anthropic".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({"model": "claude", "messages": []}));

    let err = plugin.process_request(&mut cs, &mut req).unwrap_err();
    assert_eq!(err.http_status_code(), 400);
}

/// Verifies that the streaming flag is preserved through Anthropic translation.
#[test]
fn streaming_flag_preserved() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "anthropic".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [{"role": "user", "content": "Hi"}],
        "stream": true
    }));

    plugin.process_request(&mut cs, &mut req).unwrap();
    assert_eq!(req.body["stream"], true);
}

/// Verifies multi-turn conversation with Anthropic.
#[test]
fn anthropic_multi_turn_conversation() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "anthropic".to_string());

    let body = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {"role": "system", "content": "You are a math tutor."},
            {"role": "user", "content": "What is 2+2?"},
            {"role": "assistant", "content": "2+2 equals 4."},
            {"role": "user", "content": "What about 3+3?"}
        ]
    });

    let mut req = InferenceRequest::new();
    req.set_body(body);
    plugin.process_request(&mut cs, &mut req).unwrap();

    assert_eq!(req.body["system"], "You are a math tutor.");
    let msgs = req.body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3); // user, assistant, user (system extracted)
}

/// Large payload (100KB+ prompt)
#[test]
fn large_payload_through_plugin_chain() {
    let plugin = make_plugin();
    let large_content = "x".repeat(100_000);

    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "openai".to_string());
    cs.write(state_keys::MODEL, "gpt-4o".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": large_content}]
    }));

    plugin.process_request(&mut cs, &mut req).unwrap();
    assert!(req.headers.contains_key(":path"));
}

/// Unsupported provider returns error
#[test]
fn unsupported_provider_returns_error() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "unknown-provider".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({"model": "test", "messages": [{"role": "user", "content": "hi"}]}));

    let err = plugin.process_request(&mut cs, &mut req).unwrap_err();
    assert_eq!(err.http_status_code(), 400);
}

/// Multiple system messages concatenated for Anthropic
#[test]
fn anthropic_multiple_system_messages() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "anthropic".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({
        "model": "claude",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "system", "content": "Be concise."},
            {"role": "user", "content": "Hi"}
        ]
    }));

    plugin.process_request(&mut cs, &mut req).unwrap();
    assert_eq!(req.body["system"], "You are helpful.\nBe concise.");
}

/// Azure response stripping verified in full chain
#[test]
fn azure_full_roundtrip_strips_filters() {
    let plugin = make_plugin();
    let mut cs = CycleState::new();
    cs.write(state_keys::PROVIDER, "azure-openai".to_string());
    cs.write(state_keys::MODEL, "gpt-4o".to_string());

    let mut req = InferenceRequest::new();
    req.set_body(json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hi"}]}));
    plugin.process_request(&mut cs, &mut req).unwrap();

    let mut resp = InferenceResponse::new();
    resp.inner.body = json!({
        "id": "chatcmpl-1",
        "choices": [{"index": 0, "content_filter_results": {"hate": "safe"}, "message": {"content": "Hi"}}],
        "prompt_filter_results": [{"index": 0}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    });

    plugin.process_response(&mut cs, &mut resp).unwrap();
    assert!(resp.body.get("prompt_filter_results").is_none());
    assert!(resp.body["choices"][0].get("content_filter_results").is_none());
}
