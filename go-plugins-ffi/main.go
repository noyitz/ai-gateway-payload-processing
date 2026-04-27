package main

/*
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct GoPluginChain GoPluginChain;

typedef struct {
    char *mutated_headers_json;
    char *removed_headers_json;
    uint8_t *mutated_body;
    size_t mutated_body_len;
    int32_t error_code;
    char *error_msg;
    uint64_t cycle_state_id;
} GoPluginResult;
*/
import "C"
import (
	"context"
	"encoding/json"
	"fmt"
	"runtime/cgo"
	"unsafe"

	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"
)

//export go_plugin_init
func go_plugin_init(vertexConfigJSON *C.char) C.uintptr_t {
	var vcJSON string
	if vertexConfigJSON != nil {
		vcJSON = C.GoString(vertexConfigJSON)
	}

	chain, err := initPluginChain(vcJSON)
	if err != nil {
		fmt.Printf("go_plugin_init error: %v\n", err)
		return 0
	}

	h := cgo.NewHandle(chain)
	return C.uintptr_t(h)
}

//export go_plugin_process_request
func go_plugin_process_request(
	chainHandle C.uintptr_t,
	headersJSON *C.char,
	body *C.uint8_t,
	bodyLen C.size_t,
) *C.GoPluginResult {
	chain := cgo.Handle(chainHandle).Value().(*pluginChain)

	// Parse headers
	headers := make(map[string]string)
	if headersJSON != nil {
		_ = json.Unmarshal([]byte(C.GoString(headersJSON)), &headers)
	}

	// Parse body
	var bodyMap map[string]any
	if body != nil && bodyLen > 0 {
		bodyBytes := C.GoBytes(unsafe.Pointer(body), C.int(bodyLen))
		_ = json.Unmarshal(bodyBytes, &bodyMap)
	}

	// Build InferenceRequest
	request := framework.NewInferenceRequest()
	for k, v := range headers {
		request.Headers[k] = v
	}
	request.Body = bodyMap

	// Run all request plugins
	cycleState := &framework.CycleState{}
	ctx := context.Background()

	for _, plugin := range chain.requestPlugins {
		if err := plugin.ProcessRequest(ctx, cycleState, request); err != nil {
			return makeErrorResult(err)
		}
	}

	// Store CycleState for response phase
	csID := chain.storeCycleState(cycleState)

	return makeSuccessResult(request, csID)
}

//export go_plugin_process_response
func go_plugin_process_response(
	chainHandle C.uintptr_t,
	cycleStateID C.uint64_t,
	headersJSON *C.char,
	body *C.uint8_t,
	bodyLen C.size_t,
) *C.GoPluginResult {
	chain := cgo.Handle(chainHandle).Value().(*pluginChain)

	// Retrieve stored CycleState
	cycleState := chain.loadAndDeleteCycleState(uint64(cycleStateID))
	if cycleState == nil {
		cycleState = &framework.CycleState{}
	}

	// Parse headers
	headers := make(map[string]string)
	if headersJSON != nil {
		_ = json.Unmarshal([]byte(C.GoString(headersJSON)), &headers)
	}

	// Parse body
	var bodyMap map[string]any
	if body != nil && bodyLen > 0 {
		bodyBytes := C.GoBytes(unsafe.Pointer(body), C.int(bodyLen))
		_ = json.Unmarshal(bodyBytes, &bodyMap)
	}

	// Build InferenceResponse
	response := framework.NewInferenceResponse()
	for k, v := range headers {
		response.Headers[k] = v
	}
	response.Body = bodyMap

	// Run all response plugins
	ctx := context.Background()
	for _, plugin := range chain.responsePlugins {
		if err := plugin.ProcessResponse(ctx, cycleState, response); err != nil {
			return makeErrorResult(err)
		}
	}

	return makeSuccessResponseResult(response)
}

//export go_plugin_free_result
func go_plugin_free_result(result *C.GoPluginResult) {
	if result == nil {
		return
	}
	if result.mutated_headers_json != nil {
		C.free(unsafe.Pointer(result.mutated_headers_json))
	}
	if result.removed_headers_json != nil {
		C.free(unsafe.Pointer(result.removed_headers_json))
	}
	if result.mutated_body != nil {
		C.free(unsafe.Pointer(result.mutated_body))
	}
	if result.error_msg != nil {
		C.free(unsafe.Pointer(result.error_msg))
	}
	C.free(unsafe.Pointer(result))
}

//export go_plugin_destroy
func go_plugin_destroy(chainHandle C.uintptr_t) {
	if chainHandle == 0 {
		return
	}
	h := cgo.Handle(chainHandle)
	chain := h.Value().(*pluginChain)
	chain.cancel()
	h.Delete()
}

func makeErrorResult(err error) *C.GoPluginResult {
	result := (*C.GoPluginResult)(C.calloc(1, C.size_t(unsafe.Sizeof(C.GoPluginResult{}))))

	errMsg := err.Error()
	result.error_msg = C.CString(errMsg)
	result.error_code = 500

	return result
}

func makeSuccessResult(request *framework.InferenceRequest, cycleStateID uint64) *C.GoPluginResult {
	result := (*C.GoPluginResult)(C.calloc(1, C.size_t(unsafe.Sizeof(C.GoPluginResult{}))))
	result.cycle_state_id = C.uint64_t(cycleStateID)

	// Mutated headers
	mutated := request.MutatedHeaders()
	if len(mutated) > 0 {
		j, _ := json.Marshal(mutated)
		result.mutated_headers_json = C.CString(string(j))
	}

	// Removed headers
	removed := request.RemovedHeaders()
	if len(removed) > 0 {
		j, _ := json.Marshal(removed)
		result.removed_headers_json = C.CString(string(j))
	}

	// Body mutation
	if request.BodyMutated() {
		bodyBytes, _ := json.Marshal(request.Body)
		cBody := C.CBytes(bodyBytes)
		result.mutated_body = (*C.uint8_t)(cBody)
		result.mutated_body_len = C.size_t(len(bodyBytes))
	}

	return result
}

func makeSuccessResponseResult(response *framework.InferenceResponse, ) *C.GoPluginResult {
	result := (*C.GoPluginResult)(C.calloc(1, C.size_t(unsafe.Sizeof(C.GoPluginResult{}))))

	mutated := response.MutatedHeaders()
	if len(mutated) > 0 {
		j, _ := json.Marshal(mutated)
		result.mutated_headers_json = C.CString(string(j))
	}

	removed := response.RemovedHeaders()
	if len(removed) > 0 {
		j, _ := json.Marshal(removed)
		result.removed_headers_json = C.CString(string(j))
	}

	if response.BodyMutated() {
		bodyBytes, _ := json.Marshal(response.Body)
		cBody := C.CBytes(bodyBytes)
		result.mutated_body = (*C.uint8_t)(cBody)
		result.mutated_body_len = C.size_t(len(bodyBytes))
	}

	return result
}

func main() {}
