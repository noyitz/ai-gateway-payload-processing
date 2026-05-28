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
	"testing"

	"github.com/prometheus/client_golang/prometheus/testutil"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"

	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/common/state"
)

func TestUsageTrackingPlugin_ProcessResponse(t *testing.T) {
	tests := []struct {
		name                string
		provider            string
		model               string
		user                string
		body                map[string]any
		wantRequests        float64
		wantPromptTokens    float64
		wantCompletionToken float64
	}{
		{
			name:     "no provider — skip tracking",
			provider: "",
			body:     map[string]any{},
		},
		{
			name:     "full usage data",
			provider: "openai",
			model:    "gpt-4o",
			user:     "noy",
			body: map[string]any{
				"usage": map[string]any{
					"prompt_tokens":     float64(100),
					"completion_tokens": float64(50),
					"total_tokens":      float64(150),
				},
			},
			wantRequests:        1,
			wantPromptTokens:    100,
			wantCompletionToken: 50,
		},
		{
			name:     "no usage field — count request only",
			provider: "anthropic",
			model:    "claude-3-opus",
			body: map[string]any{
				"choices": []any{},
			},
			wantRequests: 1,
		},
		{
			name:     "missing model defaults to unknown",
			provider: "vertex-oauth",
			model:    "",
			body: map[string]any{
				"usage": map[string]any{
					"prompt_tokens":     float64(10),
					"completion_tokens": float64(5),
				},
			},
			wantRequests:        1,
			wantPromptTokens:    10,
			wantCompletionToken: 5,
		},
		{
			name:     "anthropic format usage (input_tokens/output_tokens)",
			provider: "anthropic",
			model:    "claude-sonnet-4",
			user:     "yossi",
			body: map[string]any{
				"usage": map[string]any{
					"input_tokens":  float64(20),
					"output_tokens": float64(8),
				},
			},
			wantRequests:        1,
			wantPromptTokens:    20,
			wantCompletionToken: 8,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			user := test.user
			if user == "" {
				user = "anonymous"
			}
			requestsBefore := testutil.ToFloat64(requestsCounter.WithLabelValues(test.provider, effectiveModel(test.model), user))
			promptBefore := testutil.ToFloat64(promptTokensCounter.WithLabelValues(test.provider, effectiveModel(test.model), user))
			completionBefore := testutil.ToFloat64(completionTokensCounter.WithLabelValues(test.provider, effectiveModel(test.model), user))

			p := &UsageTrackingPlugin{}
			cs := framework.NewCycleState()
			if test.provider != "" {
				cs.Write(state.ProviderKey, test.provider)
			}
			if test.model != "" {
				cs.Write(state.ModelKey, test.model)
			}
			if test.user != "" {
				cs.Write(userKey, test.user)
			}

			resp := framework.NewInferenceResponse()
			resp.Body = test.body

			err := p.ProcessResponse(context.Background(), cs, resp)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			gotRequests := testutil.ToFloat64(requestsCounter.WithLabelValues(test.provider, effectiveModel(test.model), user)) - requestsBefore
			gotPrompt := testutil.ToFloat64(promptTokensCounter.WithLabelValues(test.provider, effectiveModel(test.model), user)) - promptBefore
			gotCompletion := testutil.ToFloat64(completionTokensCounter.WithLabelValues(test.provider, effectiveModel(test.model), user)) - completionBefore

			if gotRequests != test.wantRequests {
				t.Errorf("requests counter: got %v, want %v", gotRequests, test.wantRequests)
			}
			if gotPrompt != test.wantPromptTokens {
				t.Errorf("prompt_tokens counter: got %v, want %v", gotPrompt, test.wantPromptTokens)
			}
			if gotCompletion != test.wantCompletionToken {
				t.Errorf("completion_tokens counter: got %v, want %v", gotCompletion, test.wantCompletionToken)
			}
		})
	}
}

func TestUsageTrackingFactory(t *testing.T) {
	p, err := UsageTrackingFactory("my-usage", nil, nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	plugin := p.(*UsageTrackingPlugin)
	if plugin.typedName.Type != UsageTrackingPluginType {
		t.Errorf("type: got %q, want %q", plugin.typedName.Type, UsageTrackingPluginType)
	}
	if plugin.typedName.Name != "my-usage" {
		t.Errorf("name: got %q, want %q", plugin.typedName.Name, "my-usage")
	}
}

func TestCollectors(t *testing.T) {
	collectors := Collectors()
	if len(collectors) != 3 {
		t.Errorf("expected 3 collectors, got %d", len(collectors))
	}
}

func effectiveModel(model string) string {
	if model == "" {
		return "unknown"
	}
	return model
}
