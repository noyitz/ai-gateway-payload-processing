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

package awsbedrock

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTranslateRequest_BasicChat(t *testing.T) {
	tr := NewBedrockTranslator()
	body := map[string]any{
		"model":    "us.amazon.nova-lite-v1:0",
		"messages": []any{map[string]any{"role": "user", "content": "Hello"}},
	}

	translatedBody, headers, removed, err := tr.TranslateRequest(body)
	require.NoError(t, err)
	assert.Nil(t, translatedBody, "body should not be mutated")
	assert.Equal(t, "/openai/v1/chat/completions", headers[":path"])
	assert.Equal(t, "application/json", headers["content-type"])
	assert.Empty(t, removed)
}

func TestTranslateRequest_MissingModel(t *testing.T) {
	tr := NewBedrockTranslator()
	body := map[string]any{
		"messages": []any{map[string]any{"role": "user", "content": "Hi"}},
	}

	_, _, _, err := tr.TranslateRequest(body)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "model field is required")
}

func TestTranslateRequest_MissingMessages(t *testing.T) {
	tr := NewBedrockTranslator()
	body := map[string]any{
		"model": "us.amazon.nova-lite-v1:0",
	}

	_, _, _, err := tr.TranslateRequest(body)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "only Chat Completions API is supported")
}

func TestTranslateRequest_PassthroughParams(t *testing.T) {
	tr := NewBedrockTranslator()
	body := map[string]any{
		"model": "us.amazon.nova-lite-v1:0",
		"messages": []any{
			map[string]any{"role": "system", "content": "Be concise"},
			map[string]any{"role": "user", "content": "Hello"},
		},
		"temperature": 0.7,
		"max_tokens":  1000,
		"stream":      true,
	}

	translatedBody, _, _, err := tr.TranslateRequest(body)
	require.NoError(t, err)
	assert.Nil(t, translatedBody, "body should pass through unchanged")
}

func TestTranslateResponse_Passthrough(t *testing.T) {
	tr := NewBedrockTranslator()
	body := map[string]any{
		"id":     "chatcmpl-abc123",
		"object": "chat.completion",
		"model":  "us.amazon.nova-lite-v1:0",
		"choices": []any{
			map[string]any{
				"index":   0,
				"message": map[string]any{"role": "assistant", "content": "Hello!"},
			},
		},
	}

	translatedBody, err := tr.TranslateResponse(body, "us.amazon.nova-lite-v1:0")
	require.NoError(t, err)
	assert.Nil(t, translatedBody, "response should pass through unchanged")
}

func TestRegionFromEndpoint(t *testing.T) {
	tests := []struct {
		endpoint string
		want     string
	}{
		{"bedrock-runtime.us-east-1.amazonaws.com", "us-east-1"},
		{"bedrock-runtime.us-west-2.amazonaws.com", "us-west-2"},
		{"bedrock-runtime.eu-west-1.amazonaws.com", "eu-west-1"},
		{"some-other-host.com", "us-east-1"},
		{"", "us-east-1"},
	}

	for _, tt := range tests {
		t.Run(tt.endpoint, func(t *testing.T) {
			assert.Equal(t, tt.want, RegionFromEndpoint(tt.endpoint))
		})
	}
}
