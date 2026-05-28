/*
Copyright 2026 The opendatahub.io Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package usage_tracking

import (
	"context"
	"encoding/json"

	"github.com/prometheus/client_golang/prometheus"
	"sigs.k8s.io/controller-runtime/pkg/log"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"
	logutil "sigs.k8s.io/gateway-api-inference-extension/pkg/common/observability/logging"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/epp/framework/interface/plugin"

	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/common/state"
)

const (
	UsageTrackingPluginType = "usage-tracking"
	userKey                 = "usage-tracking-user"
)

var _ framework.RequestProcessor = &UsageTrackingPlugin{}
var _ framework.ResponseProcessor = &UsageTrackingPlugin{}

var (
	promptTokensCounter = prometheus.NewCounterVec(
		prometheus.CounterOpts{
			Name: "ipp_usage_prompt_tokens_total",
			Help: "Total prompt tokens consumed, by provider, model, and user.",
		},
		[]string{"provider", "model", "user"},
	)

	completionTokensCounter = prometheus.NewCounterVec(
		prometheus.CounterOpts{
			Name: "ipp_usage_completion_tokens_total",
			Help: "Total completion tokens consumed, by provider, model, and user.",
		},
		[]string{"provider", "model", "user"},
	)

	requestsCounter = prometheus.NewCounterVec(
		prometheus.CounterOpts{
			Name: "ipp_usage_requests_total",
			Help: "Total inference requests, by provider, model, and user.",
		},
		[]string{"provider", "model", "user"},
	)
)

// Collectors returns all Prometheus collectors defined by this plugin.
func Collectors() []prometheus.Collector {
	return []prometheus.Collector{promptTokensCounter, completionTokensCounter, requestsCounter}
}

// UsageTrackingFactory defines the factory function for UsageTrackingPlugin.
func UsageTrackingFactory(name string, _ json.RawMessage, _ framework.Handle) (framework.BBRPlugin, error) {
	return (&UsageTrackingPlugin{
		typedName: plugin.TypedName{
			Type: UsageTrackingPluginType,
			Name: UsageTrackingPluginType,
		},
	}).WithName(name), nil
}

// UsageTrackingPlugin records token usage from inference responses as Prometheus counters.
type UsageTrackingPlugin struct {
	typedName plugin.TypedName
}

func (p *UsageTrackingPlugin) TypedName() plugin.TypedName {
	return p.typedName
}

func (p *UsageTrackingPlugin) WithName(name string) *UsageTrackingPlugin {
	p.typedName.Name = name
	return p
}

// ProcessRequest captures the user identity from the X-MaaS-User header and stores it
// in CycleState for use during response processing.
func (p *UsageTrackingPlugin) ProcessRequest(ctx context.Context, cycleState *framework.CycleState, request *framework.InferenceRequest) error {
	log.FromContext(ctx).V(logutil.VERBOSE).Info("usage-tracking request headers", "headers", request.Headers)
	if user, ok := request.Headers["x-maas-user"]; ok && user != "" {
		cycleState.Write(userKey, user)
	}
	return nil
}

// ProcessResponse reads token usage from the response body and records it as Prometheus
// counter increments. Supports both OpenAI and Anthropic response formats.
func (p *UsageTrackingPlugin) ProcessResponse(ctx context.Context, cycleState *framework.CycleState, response *framework.InferenceResponse) error {
	providerName, err := framework.ReadCycleStateKey[string](cycleState, state.ProviderKey)
	if err != nil || providerName == "" {
		return nil
	}

	modelName, _ := framework.ReadCycleStateKey[string](cycleState, state.ModelKey)
	if modelName == "" {
		modelName = "unknown"
	}

	userName, _ := framework.ReadCycleStateKey[string](cycleState, userKey)
	if userName == "" {
		userName = "anonymous"
	}

	requestsCounter.WithLabelValues(providerName, modelName, userName).Inc()

	usage, ok := response.Body["usage"].(map[string]any)
	if !ok {
		log.FromContext(ctx).V(logutil.VERBOSE).Info("no usage data in response", "provider", providerName, "model", modelName)
		return nil
	}

	if v, ok := toFloat64(usage["prompt_tokens"]); ok {
		promptTokensCounter.WithLabelValues(providerName, modelName, userName).Add(v)
	} else if v, ok := toFloat64(usage["input_tokens"]); ok {
		promptTokensCounter.WithLabelValues(providerName, modelName, userName).Add(v)
	}
	if v, ok := toFloat64(usage["completion_tokens"]); ok {
		completionTokensCounter.WithLabelValues(providerName, modelName, userName).Add(v)
	} else if v, ok := toFloat64(usage["output_tokens"]); ok {
		completionTokensCounter.WithLabelValues(providerName, modelName, userName).Add(v)
	}

	return nil
}

func toFloat64(v any) (float64, bool) {
	switch n := v.(type) {
	case float64:
		return n, true
	case int:
		return float64(n), true
	case int64:
		return float64(n), true
	default:
		return 0, false
	}
}
