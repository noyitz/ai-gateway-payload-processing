use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ipp_framework::cycle_state::CycleState;
use ipp_framework::inference_message::{InferenceRequest, InferenceResponse};
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use ipp_framework::state_keys;
use ipp_translators::anthropic::AnthropicTranslator;
use ipp_translators::api_translation_plugin::{ApiTranslationPlugin, VertexOpenAiConfig};
use ipp_translators::azure_openai::AzureOpenAiTranslator;
use ipp_translators::openai::OpenAiTranslator;
use ipp_translators::translator::Translator;
use serde_json::Value;

fn load_fixture(name: &str) -> Value {
    let path = format!(
        "{}/../../testdata/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let data = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", path, e));
    serde_json::from_str(&data).unwrap()
}

fn bench_anthropic_request_basic(c: &mut Criterion) {
    let translator = AnthropicTranslator;
    let body = load_fixture("openai_basic_request.json");

    c.bench_function("anthropic_request_basic", |b| {
        b.iter(|| {
            let result = translator.translate_request(black_box(&body));
            black_box(result).unwrap();
        })
    });
}

fn bench_anthropic_request_complex(c: &mut Criterion) {
    let translator = AnthropicTranslator;
    let body = load_fixture("openai_complex_request.json");

    c.bench_function("anthropic_request_complex", |b| {
        b.iter(|| {
            let result = translator.translate_request(black_box(&body));
            black_box(result).unwrap();
        })
    });
}

fn bench_anthropic_response_text(c: &mut Criterion) {
    let translator = AnthropicTranslator;
    let body_orig = load_fixture("anthropic_response_text.json");

    c.bench_function("anthropic_response_text", |b| {
        b.iter(|| {
            let mut body = body_orig.clone();
            translator.translate_response(black_box(&mut body), "claude-3-5-sonnet-20241022").unwrap();
            black_box(body);
        })
    });
}

fn bench_anthropic_response_tool_use(c: &mut Criterion) {
    let translator = AnthropicTranslator;
    let body_orig = load_fixture("anthropic_response_tool_use.json");

    c.bench_function("anthropic_response_tool_use", |b| {
        b.iter(|| {
            let mut body = body_orig.clone();
            translator.translate_response(black_box(&mut body), "claude-3-5-sonnet-20241022").unwrap();
            black_box(body);
        })
    });
}

fn bench_openai_passthrough(c: &mut Criterion) {
    let translator = OpenAiTranslator;
    let body = load_fixture("openai_basic_request.json");

    c.bench_function("openai_passthrough", |b| {
        b.iter(|| {
            let result = translator.translate_request(black_box(&body));
            black_box(result).unwrap();
        })
    });
}

fn bench_azure_response_strip(c: &mut Criterion) {
    let translator = AzureOpenAiTranslator::new();
    let body_orig = load_fixture("azure_response_with_filters.json");

    c.bench_function("azure_response_strip", |b| {
        b.iter(|| {
            let mut body = body_orig.clone();
            translator.translate_response(black_box(&mut body), "gpt-4o").unwrap();
            black_box(body);
        })
    });
}

fn bench_full_plugin_chain_anthropic(c: &mut Criterion) {
    let plugin = ApiTranslationPlugin::new(Some(VertexOpenAiConfig {
        project: "test-project".to_string(),
        location: "us-central1".to_string(),
        endpoint: "openapi".to_string(),
    }))
    .unwrap();
    let body = load_fixture("openai_complex_request.json");

    c.bench_function("full_plugin_chain_anthropic", |b| {
        b.iter(|| {
            let mut cs = CycleState::new();
            cs.write(state_keys::PROVIDER, "anthropic".to_string());
            cs.write(state_keys::MODEL, "claude-3-5-sonnet-20241022".to_string());

            let mut req = InferenceRequest::new();
            req.inner.body = body.clone();
            plugin.process_request(&mut cs, &mut req).unwrap();
            black_box(&req);
        })
    });
}

fn bench_full_plugin_chain_openai(c: &mut Criterion) {
    let plugin = ApiTranslationPlugin::new(None).unwrap();
    let body = load_fixture("openai_basic_request.json");

    c.bench_function("full_plugin_chain_openai", |b| {
        b.iter(|| {
            let mut cs = CycleState::new();
            cs.write(state_keys::PROVIDER, "openai".to_string());
            cs.write(state_keys::MODEL, "gpt-4o".to_string());

            let mut req = InferenceRequest::new();
            req.inner.body = body.clone();
            plugin.process_request(&mut cs, &mut req).unwrap();
            black_box(&req);
        })
    });
}

fn bench_full_roundtrip_anthropic(c: &mut Criterion) {
    let plugin = ApiTranslationPlugin::new(Some(VertexOpenAiConfig {
        project: "test-project".to_string(),
        location: "us-central1".to_string(),
        endpoint: "openapi".to_string(),
    }))
    .unwrap();
    let req_body = load_fixture("openai_complex_request.json");
    let resp_body = load_fixture("anthropic_response_tool_use.json");

    c.bench_function("full_roundtrip_anthropic", |b| {
        b.iter(|| {
            let mut cs = CycleState::new();
            cs.write(state_keys::PROVIDER, "anthropic".to_string());
            cs.write(state_keys::MODEL, "claude-3-5-sonnet-20241022".to_string());

            let mut req = InferenceRequest::new();
            req.inner.body = req_body.clone();
            plugin.process_request(&mut cs, &mut req).unwrap();

            let mut resp = InferenceResponse::new();
            resp.inner.body = resp_body.clone();
            plugin.process_response(&mut cs, &mut resp).unwrap();
            black_box(&resp);
        })
    });
}

fn bench_json_parse_serialize(c: &mut Criterion) {
    let raw = std::fs::read_to_string(format!(
        "{}/../../testdata/openai_complex_request.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let raw_bytes = raw.as_bytes();

    c.bench_function("json_parse_serialize_complex", |b| {
        b.iter(|| {
            let value: Value = serde_json::from_slice(black_box(raw_bytes)).unwrap();
            let output = serde_json::to_vec(&value).unwrap();
            black_box(output);
        })
    });
}

criterion_group!(
    benches,
    bench_anthropic_request_basic,
    bench_anthropic_request_complex,
    bench_anthropic_response_text,
    bench_anthropic_response_tool_use,
    bench_openai_passthrough,
    bench_azure_response_strip,
    bench_full_plugin_chain_anthropic,
    bench_full_plugin_chain_openai,
    bench_full_roundtrip_anthropic,
    bench_json_parse_serialize,
);
criterion_main!(benches);
