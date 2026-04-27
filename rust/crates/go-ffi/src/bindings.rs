use std::os::raw::c_char;

#[repr(C)]
pub struct GoPluginResult {
    pub mutated_headers_json: *mut c_char,
    pub removed_headers_json: *mut c_char,
    pub mutated_body: *mut u8,
    pub mutated_body_len: usize,
    pub error_code: i32,
    pub error_msg: *mut c_char,
    pub cycle_state_id: u64,
}

extern "C" {
    pub fn go_plugin_init(vertex_config_json: *const c_char) -> usize;

    pub fn go_plugin_process_request(
        chain_handle: usize,
        headers_json: *const c_char,
        body: *const u8,
        body_len: usize,
    ) -> *mut GoPluginResult;

    pub fn go_plugin_process_response(
        chain_handle: usize,
        cycle_state_id: u64,
        headers_json: *const c_char,
        body: *const u8,
        body_len: usize,
    ) -> *mut GoPluginResult;

    pub fn go_plugin_free_result(result: *mut GoPluginResult);

    pub fn go_plugin_destroy(chain_handle: usize);
}
