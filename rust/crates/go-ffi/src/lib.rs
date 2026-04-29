mod bindings;

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::Arc;

use ipp_framework::cycle_state::CycleState;
use ipp_framework::error::PluginError;
use ipp_framework::inference_message::{InferenceRequest, InferenceResponse};
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use tracing::{info, warn};

const GO_CYCLE_STATE_ID_KEY: &str = "go_cycle_state_id";

pub struct GoPluginBridge {
    chain_handle: usize,
}

unsafe impl Send for GoPluginBridge {}
unsafe impl Sync for GoPluginBridge {}

impl GoPluginBridge {
    pub fn new(vertex_config: Option<&str>) -> Result<Self, PluginError> {
        let config_ptr = vertex_config
            .map(|c| CString::new(c).unwrap())
            .map(|c| c.into_raw() as *const _)
            .unwrap_or(ptr::null());

        let handle = unsafe { bindings::go_plugin_init(config_ptr) };

        // Free the CString if we allocated one
        if !config_ptr.is_null() {
            unsafe { let _ = CString::from_raw(config_ptr as *mut _); }
        }

        if handle == 0 {
            return Err(PluginError::internal("failed to initialize Go plugin chain"));
        }

        info!("Go plugin chain initialized");
        Ok(Self { chain_handle: handle })
    }
}

impl Drop for GoPluginBridge {
    fn drop(&mut self) {
        if self.chain_handle != 0 {
            unsafe { bindings::go_plugin_destroy(self.chain_handle) };
            self.chain_handle = 0;
        }
    }
}

impl RequestProcessor for GoPluginBridge {
    fn name(&self) -> &str {
        "go-plugins-bridge"
    }

    fn process_request(
        &self,
        cycle_state: &mut CycleState,
        request: &mut InferenceRequest,
    ) -> Result<(), PluginError> {
        let headers_json = serde_json::to_string(&request.headers)
            .map_err(|e| PluginError::internal(format!("failed to marshal headers: {e}")))?;
        let body_bytes = serde_json::to_vec(&request.body)
            .map_err(|e| PluginError::internal(format!("failed to marshal body: {e}")))?;

        let c_headers = CString::new(headers_json)
            .map_err(|e| PluginError::internal(format!("headers contain null byte: {e}")))?;

        let result = unsafe {
            bindings::go_plugin_process_request(
                self.chain_handle,
                c_headers.as_ptr(),
                body_bytes.as_ptr(),
                body_bytes.len(),
            )
        };

        if result.is_null() {
            return Err(PluginError::internal("go_plugin_process_request returned null"));
        }

        // Read cycle_state_id before processing result (in case result has an error)
        let cs_id = unsafe { (*result).cycle_state_id };

        let outcome = unsafe { apply_result(result, request) };

        // Store cycle_state_id for response phase
        cycle_state.write(GO_CYCLE_STATE_ID_KEY, cs_id);

        unsafe { bindings::go_plugin_free_result(result) };

        outcome
    }
}

impl ResponseProcessor for GoPluginBridge {
    fn name(&self) -> &str {
        "go-plugins-bridge"
    }

    fn process_response(
        &self,
        cycle_state: &mut CycleState,
        response: &mut InferenceResponse,
    ) -> Result<(), PluginError> {
        let cs_id = cycle_state
            .try_read::<u64>(GO_CYCLE_STATE_ID_KEY)
            .copied()
            .unwrap_or(0);

        let headers_json = serde_json::to_string(&response.headers)
            .map_err(|e| PluginError::internal(format!("failed to marshal headers: {e}")))?;
        let body_bytes = serde_json::to_vec(&response.body)
            .map_err(|e| PluginError::internal(format!("failed to marshal body: {e}")))?;

        let c_headers = CString::new(headers_json)
            .map_err(|e| PluginError::internal(format!("headers contain null byte: {e}")))?;

        let result = unsafe {
            bindings::go_plugin_process_response(
                self.chain_handle,
                cs_id,
                c_headers.as_ptr(),
                body_bytes.as_ptr(),
                body_bytes.len(),
            )
        };

        if result.is_null() {
            return Err(PluginError::internal("go_plugin_process_response returned null"));
        }

        let outcome = unsafe { apply_response_result(result, response) };
        unsafe { bindings::go_plugin_free_result(result) };

        outcome
    }
}

unsafe fn apply_result(
    result: *mut bindings::GoPluginResult,
    request: &mut InferenceRequest,
) -> Result<(), PluginError> {
    let r = &*result;

    if r.error_code != 0 {
        let msg = if r.error_msg.is_null() {
            "unknown Go plugin error".to_string()
        } else {
            CStr::from_ptr(r.error_msg).to_string_lossy().to_string()
        };
        return match r.error_code {
            400 => Err(PluginError::bad_request(msg)),
            404 => Err(PluginError::not_found(msg)),
            _ => Err(PluginError::internal(msg)),
        };
    }

    if !r.mutated_headers_json.is_null() {
        let json_str = CStr::from_ptr(r.mutated_headers_json).to_string_lossy();
        if let Ok(headers) = serde_json::from_str::<HashMap<String, String>>(&json_str) {
            for (k, v) in headers {
                request.set_header(k, v);
            }
        }
    }

    if !r.removed_headers_json.is_null() {
        let json_str = CStr::from_ptr(r.removed_headers_json).to_string_lossy();
        if let Ok(removed) = serde_json::from_str::<Vec<String>>(&json_str) {
            for k in removed {
                request.remove_header(&k);
            }
        }
    }

    if !r.mutated_body.is_null() && r.mutated_body_len > 0 {
        let body_slice = std::slice::from_raw_parts(r.mutated_body, r.mutated_body_len);
        if let Ok(body) = serde_json::from_slice(body_slice) {
            request.set_body(body);
        }
    }

    Ok(())
}

unsafe fn apply_response_result(
    result: *mut bindings::GoPluginResult,
    response: &mut InferenceResponse,
) -> Result<(), PluginError> {
    let r = &*result;

    if r.error_code != 0 {
        let msg = if r.error_msg.is_null() {
            "unknown Go plugin error".to_string()
        } else {
            CStr::from_ptr(r.error_msg).to_string_lossy().to_string()
        };
        return Err(PluginError::internal(msg));
    }

    if !r.mutated_headers_json.is_null() {
        let json_str = CStr::from_ptr(r.mutated_headers_json).to_string_lossy();
        if let Ok(headers) = serde_json::from_str::<HashMap<String, String>>(&json_str) {
            for (k, v) in headers {
                response.set_header(k, v);
            }
        }
    }

    if !r.removed_headers_json.is_null() {
        let json_str = CStr::from_ptr(r.removed_headers_json).to_string_lossy();
        if let Ok(removed) = serde_json::from_str::<Vec<String>>(&json_str) {
            for k in removed {
                response.remove_header(&k);
            }
        }
    }

    if !r.mutated_body.is_null() && r.mutated_body_len > 0 {
        let body_slice = std::slice::from_raw_parts(r.mutated_body, r.mutated_body_len);
        if let Ok(body) = serde_json::from_slice(body_slice) {
            response.set_body(body);
        }
    }

    Ok(())
}
