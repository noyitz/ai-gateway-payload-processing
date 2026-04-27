package main

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"sync/atomic"

	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/cache"
	metricsserver "sigs.k8s.io/controller-runtime/pkg/metrics/server"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"

	api_translation "github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/api-translation"
	apikey_injection "github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/apikey-injection"
	provider_resolver "github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/model-provider-resolver"

	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/plugins/bodyfieldtoheader"
)

type pluginChain struct {
	requestPlugins  []framework.RequestProcessor
	responsePlugins []framework.ResponseProcessor
	cancel          context.CancelFunc
	cycleStates     sync.Map
	nextID          atomic.Uint64
}

func (pc *pluginChain) storeCycleState(cs *framework.CycleState) uint64 {
	id := pc.nextID.Add(1)
	pc.cycleStates.Store(id, cs)
	return id
}

func (pc *pluginChain) loadAndDeleteCycleState(id uint64) *framework.CycleState {
	val, ok := pc.cycleStates.LoadAndDelete(id)
	if !ok {
		return nil
	}
	return val.(*framework.CycleState)
}

func initPluginChain(vertexConfigJSON string) (*pluginChain, error) {
	cfg, err := ctrl.GetConfig()
	if err != nil {
		return nil, fmt.Errorf("failed to get K8s config: %w", err)
	}

	mgr, err := ctrl.NewManager(cfg, ctrl.Options{
		Cache: cache.Options{},
		Metrics: metricsserver.Options{
			BindAddress: "0", // disable metrics server
		},
	})
	if err != nil {
		return nil, fmt.Errorf("failed to create manager: %w", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	handle := framework.NewBbrHandle(ctx, mgr)

	var requestPlugins []framework.RequestProcessor
	var responsePlugins []framework.ResponseProcessor

	// Plugin 1: body-field-to-header
	bfhConfig := json.RawMessage(`{"fieldName":"model","headerName":"X-Gateway-Model-Name"}`)
	bfhPlugin, err := bodyfieldtoheader.BodyFieldToHeaderPluginFactory("model-extractor", bfhConfig, handle)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create body-field-to-header plugin: %w", err)
	}
	if rp, ok := bfhPlugin.(framework.RequestProcessor); ok {
		requestPlugins = append(requestPlugins, rp)
	}

	// Plugin 2: model-provider-resolver
	mprPlugin, err := provider_resolver.ModelProviderResolverFactory("model-provider-resolver", nil, handle)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create model-provider-resolver plugin: %w", err)
	}
	if rp, ok := mprPlugin.(framework.RequestProcessor); ok {
		requestPlugins = append(requestPlugins, rp)
	}

	// Plugin 3: api-translation
	var atConfig json.RawMessage
	if vertexConfigJSON != "" {
		atConfig = json.RawMessage(fmt.Sprintf(`{"vertexOpenAI":%s}`, vertexConfigJSON))
	}
	atPlugin, err := api_translation.APITranslationFactory("api-translation", atConfig, handle)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create api-translation plugin: %w", err)
	}
	if rp, ok := atPlugin.(framework.RequestProcessor); ok {
		requestPlugins = append(requestPlugins, rp)
	}
	if rp, ok := atPlugin.(framework.ResponseProcessor); ok {
		responsePlugins = append(responsePlugins, rp)
	}

	// Plugin 4: apikey-injection
	akiPlugin, err := apikey_injection.APIKeyInjectionFactory("apikey-injection", nil, handle)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create apikey-injection plugin: %w", err)
	}
	if rp, ok := akiPlugin.(framework.RequestProcessor); ok {
		requestPlugins = append(requestPlugins, rp)
	}

	// Start manager in background
	go func() {
		if err := mgr.Start(ctx); err != nil {
			fmt.Printf("manager error: %v\n", err)
		}
	}()

	// Wait for cache sync
	if !mgr.GetCache().WaitForCacheSync(ctx) {
		cancel()
		return nil, fmt.Errorf("failed to sync caches")
	}

	return &pluginChain{
		requestPlugins:  requestPlugins,
		responsePlugins: responsePlugins,
		cancel:          cancel,
	}, nil
}
