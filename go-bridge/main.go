package main

import (
	"encoding/json"
	"fmt"
	"os"

	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/gateway-api-inference-extension/cmd/bbr/runner"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/epp/framework/interface/plugin"
)

const (
	RustBridgePluginType = "rust-bridge"
)

// RustBridgeFactory is the BBR plugin factory for the Rust FFI bridge.
// It reads optional vertexOpenAI config from the plugin JSON config.
func RustBridgeFactory(name string, rawConfig json.RawMessage, _ framework.Handle) (framework.BBRPlugin, error) {
	vertexConfig := ""
	if len(rawConfig) > 0 {
		var config struct {
			VertexOpenAI *struct {
				Project  string `json:"project"`
				Location string `json:"location"`
				Endpoint string `json:"endpoint"`
			} `json:"vertexOpenAI,omitempty"`
		}
		if err := json.Unmarshal(rawConfig, &config); err == nil && config.VertexOpenAI != nil {
			vBytes, _ := json.Marshal(config.VertexOpenAI)
			vertexConfig = string(vBytes)
		}
	}

	chain, err := NewRustPluginChain(vertexConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create rust-bridge plugin: %w", err)
	}
	return chain.WithName(name), nil
}

func main() {
	// Register the Rust FFI bridge as a single BBR plugin.
	// This replaces all Go plugins (body-field-to-header, model-provider-resolver,
	// api-translation, apikey-injection) with the Rust implementation.
	framework.Register(RustBridgePluginType, RustBridgeFactory)

	if err := runner.NewRunner().
		WithExecutableName("bbr-hybrid").
		Run(ctrl.SetupSignalHandler()); err != nil {
		os.Exit(1)
	}
}

// Compile-time interface checks
var _ framework.RequestProcessor = &RustPluginChain{}
var _ framework.ResponseProcessor = &RustPluginChain{}

// BBRPlugin interface — TypedName is already implemented on RustPluginChain
var _ interface {
	TypedName() plugin.TypedName
} = &RustPluginChain{}
