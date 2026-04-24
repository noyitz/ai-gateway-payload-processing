use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::sync::Arc;

use ipp_framework::cycle_state::CycleState;
use ipp_framework::inference_message::{InferenceRequest, InferenceResponse};
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use ipp_k8s_plugins::apikey_injection::secret_store::SecretStore;
use ipp_k8s_plugins::apikey_injection::ApiKeyInjectionPlugin;
use ipp_k8s_plugins::body_field_to_header::BodyFieldToHeaderPlugin;
use ipp_k8s_plugins::model_provider_resolver::model_store::ModelInfoStore;
use ipp_k8s_plugins::model_provider_resolver::ModelProviderResolverPlugin;
use ipp_translators::api_translation_plugin::{ApiTranslationPlugin, VertexOpenAiConfig};

struct PluginChain {
    request_plugins: Vec<Box<dyn RequestProcessor>>,
    response_plugins: Vec<Box<dyn ResponseProcessor>>,
    _runtime: tokio::runtime::Runtime,
}

/// Result of processing a request or response through the plugin chain.
#[repr(C)]
pub struct IppResult {
    /// JSON object of headers to set: {"key": "value", ...}
    pub mutated_headers_json: *mut c_char,
    /// JSON array of header names to remove: ["key1", "key2", ...]
    pub removed_headers_json: *mut c_char,
    /// Mutated body bytes (NULL if body unchanged)
    pub mutated_body: *mut u8,
    /// Length of mutated_body
    pub mutated_body_len: usize,
    /// Error code: 0=ok, 400=bad_request, 404=not_found, 500=internal
    pub error_code: i32,
    /// Error message (NULL if no error)
    pub error_msg: *mut c_char,
}

/// Initialize the Rust plugin chain.
///
/// Spawns a tokio runtime, starts kube-rs reconcilers if K8s is available,
/// and builds the full plugin chain.
///
/// `vertex_config_json` is optional JSON: {"project":"...","location":"...","endpoint":"..."}
/// Pass NULL to skip vertex-openai provider.
///
/// Returns an opaque handle to the plugin chain, or NULL on failure.
#[no_mangle]
pub extern "C" fn ipp_init(vertex_config_json: *const c_char) -> *mut PluginChain {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .try_init();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return ptr::null_mut(),
    };

    let vertex_config = if !vertex_config_json.is_null() {
        let json_str = unsafe { CStr::from_ptr(vertex_config_json) }
            .to_string_lossy()
            .to_string();
        serde_json::from_str::<serde_json::Value>(&json_str)
            .ok()
            .map(|v| VertexOpenAiConfig {
                project: v["project"].as_str().unwrap_or("").to_string(),
                location: v["location"].as_str().unwrap_or("").to_string(),
                endpoint: v["endpoint"].as_str().unwrap_or("").to_string(),
            })
    } else {
        None
    };

    let model_store = ModelInfoStore::new();
    let secret_store = SecretStore::new();

    // Start kube-rs reconcilers on the runtime
    let kube_client = runtime.block_on(async { kube::Client::try_default().await.ok() });

    if let Some(client) = kube_client {
        let ms = model_store.clone();
        let c = client.clone();
        runtime.spawn(async move {
            ipp_k8s_plugins::model_provider_resolver::reconciler::run_external_model_watcher(c, ms)
                .await;
        });

        let ss = secret_store.clone();
        runtime.spawn(async move {
            ipp_k8s_plugins::apikey_injection::reconciler::run_secret_watcher(client, ss).await;
        });
    }

    let body_to_header = match BodyFieldToHeaderPlugin::new("model", "X-Gateway-Model-Name") {
        Ok(p) => p,
        Err(_) => return ptr::null_mut(),
    };

    let api_translation = match ApiTranslationPlugin::new(vertex_config.clone()) {
        Ok(p) => p,
        Err(_) => return ptr::null_mut(),
    };

    let api_translation_resp = match ApiTranslationPlugin::new(vertex_config) {
        Ok(p) => p,
        Err(_) => return ptr::null_mut(),
    };

    let request_plugins: Vec<Box<dyn RequestProcessor>> = vec![
        Box::new(body_to_header),
        Box::new(ModelProviderResolverPlugin::new(model_store)),
        Box::new(api_translation),
        Box::new(ApiKeyInjectionPlugin::new(secret_store)),
    ];

    let response_plugins: Vec<Box<dyn ResponseProcessor>> = vec![Box::new(api_translation_resp)];

    let chain = Box::new(PluginChain {
        request_plugins,
        response_plugins,
        _runtime: runtime,
    });

    Box::into_raw(chain)
}

/// Process a request through the full plugin chain.
///
/// `headers_json`: JSON object of request headers {"key": "value", ...}
/// `body`: request body bytes
/// `body_len`: length of body
///
/// Returns a pointer to IppResult. Caller must free with `ipp_free_result`.
#[no_mangle]
pub extern "C" fn ipp_process_request(
    chain: *mut PluginChain,
    headers_json: *const c_char,
    body: *const u8,
    body_len: usize,
) -> *mut IppResult {
    let chain = unsafe {
        match chain.as_ref() {
            Some(c) => c,
            None => return make_error_result(500, "null chain pointer"),
        }
    };

    let headers = parse_headers_json(headers_json);
    let body_slice = if body.is_null() || body_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(body, body_len) }
    };

    let body_value = serde_json::from_slice(body_slice).unwrap_or(serde_json::Value::Null);

    let mut request = InferenceRequest::with_headers_and_body(headers, body_value);
    let mut cycle_state = CycleState::new();

    for plugin in &chain.request_plugins {
        if let Err(e) = plugin.process_request(&mut cycle_state, &mut request) {
            return make_error_result(e.http_status_code() as i32, &e.msg);
        }
    }

    make_success_result(&request.inner)
}

/// Process a response through the full plugin chain.
///
/// Uses the CycleState from the last request (maintained internally).
/// Note: In the FFI model, each call creates a fresh CycleState. For proper
/// request-response correlation, the Go bridge must pass provider info via headers.
#[no_mangle]
pub extern "C" fn ipp_process_response(
    chain: *mut PluginChain,
    headers_json: *const c_char,
    body: *const u8,
    body_len: usize,
) -> *mut IppResult {
    let chain = unsafe {
        match chain.as_ref() {
            Some(c) => c,
            None => return make_error_result(500, "null chain pointer"),
        }
    };

    let headers = parse_headers_json(headers_json);
    let body_slice = if body.is_null() || body_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(body, body_len) }
    };

    let body_value = serde_json::from_slice(body_slice).unwrap_or(serde_json::Value::Null);

    let mut response = InferenceResponse::with_headers_and_body(headers, body_value);
    let mut cycle_state = CycleState::new();

    for plugin in &chain.response_plugins {
        if let Err(e) = plugin.process_response(&mut cycle_state, &mut response) {
            return make_error_result(e.http_status_code() as i32, &e.msg);
        }
    }

    make_success_result(&response.inner)
}

/// Free an IppResult returned by ipp_process_request or ipp_process_response.
#[no_mangle]
pub extern "C" fn ipp_free_result(result: *mut IppResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        if !r.mutated_headers_json.is_null() {
            let _ = CString::from_raw(r.mutated_headers_json);
        }
        if !r.removed_headers_json.is_null() {
            let _ = CString::from_raw(r.removed_headers_json);
        }
        if !r.mutated_body.is_null() {
            let _ = Vec::from_raw_parts(r.mutated_body, r.mutated_body_len, r.mutated_body_len);
        }
        if !r.error_msg.is_null() {
            let _ = CString::from_raw(r.error_msg);
        }
    }
}

/// Destroy the plugin chain and shut down the tokio runtime.
#[no_mangle]
pub extern "C" fn ipp_destroy(chain: *mut PluginChain) {
    if !chain.is_null() {
        unsafe {
            let _ = Box::from_raw(chain);
        }
    }
}

// --- Internal helpers ---

fn parse_headers_json(headers_json: *const c_char) -> HashMap<String, String> {
    if headers_json.is_null() {
        return HashMap::new();
    }
    let json_str = unsafe { CStr::from_ptr(headers_json) }
        .to_string_lossy()
        .to_string();
    serde_json::from_str::<HashMap<String, String>>(&json_str).unwrap_or_default()
}

fn make_error_result(code: i32, msg: &str) -> *mut IppResult {
    Box::into_raw(Box::new(IppResult {
        mutated_headers_json: ptr::null_mut(),
        removed_headers_json: ptr::null_mut(),
        mutated_body: ptr::null_mut(),
        mutated_body_len: 0,
        error_code: code,
        error_msg: CString::new(msg).unwrap_or_default().into_raw(),
    }))
}

fn make_success_result(
    msg: &ipp_framework::inference_message::InferenceMessage,
) -> *mut IppResult {
    let mutated_headers = msg.mutated_headers();
    let removed_headers = msg.removed_headers();

    let headers_json = if mutated_headers.is_empty() {
        ptr::null_mut()
    } else {
        CString::new(serde_json::to_string(mutated_headers).unwrap_or_default())
            .unwrap_or_default()
            .into_raw()
    };

    let removed_json = if removed_headers.is_empty() {
        ptr::null_mut()
    } else {
        CString::new(serde_json::to_string(&removed_headers).unwrap_or_default())
            .unwrap_or_default()
            .into_raw()
    };

    let (body_ptr, body_len) = if msg.body_mutated() {
        let bytes = serde_json::to_vec(&msg.body).unwrap_or_default();
        let len = bytes.len();
        let ptr = bytes.leak().as_mut_ptr();
        (ptr, len)
    } else {
        (ptr::null_mut(), 0)
    };

    Box::into_raw(Box::new(IppResult {
        mutated_headers_json: headers_json,
        removed_headers_json: removed_json,
        mutated_body: body_ptr,
        mutated_body_len: body_len,
        error_code: 0,
        error_msg: ptr::null_mut(),
    }))
}
