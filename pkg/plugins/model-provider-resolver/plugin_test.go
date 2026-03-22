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

package model_provider_resolver

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"k8s.io/apimachinery/pkg/types"
	"sigs.k8s.io/gateway-api-inference-extension/pkg/bbr/framework"

	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/external-model/provider"
	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/external-model/state"
)

func TestProcessRequest_ModelResolved(t *testing.T) {
	store := newModelInfoStore()
	store.setModelInfo("claude-sonnet", ModelInfo{
		provider:               provider.Anthropic,
		credentialRefName:      "anthropic-key",
		credentialRefNamespace: "llm",
	}, types.NamespacedName{Name: "claude-sonnet", Namespace: "llm"})

	p := &ModelProviderResolverPlugin{store: store}
	cs := framework.NewCycleState()
	req := framework.NewInferenceRequest()
	req.Body["model"] = "claude-sonnet"

	err := p.ProcessRequest(context.Background(), cs, req)
	require.NoError(t, err)

	actualProvider, provErr := framework.ReadCycleStateKey[string](cs, state.ProviderKey)
	assert.NoError(t, provErr)
	assert.Equal(t, provider.Anthropic, actualProvider)

	credName, _ := framework.ReadCycleStateKey[string](cs, "credential-ref-name")
	assert.Equal(t, "anthropic-key", credName)

	credNS, _ := framework.ReadCycleStateKey[string](cs, "credential-ref-namespace")
	assert.Equal(t, "llm", credNS)
}

func TestProcessRequest_ModelNotFound(t *testing.T) {
	store := newModelInfoStore()
	p := &ModelProviderResolverPlugin{store: store}
	cs := framework.NewCycleState()
	req := framework.NewInferenceRequest()
	req.Body["model"] = "unknown-model"

	err := p.ProcessRequest(context.Background(), cs, req)
	assert.NoError(t, err)

	_, provErr := framework.ReadCycleStateKey[string](cs, state.ProviderKey)
	assert.Error(t, provErr) // not found in CycleState
}

func TestProcessRequest_InternalModel(t *testing.T) {
	// Internal models are not added to the store (reconciler skips kind != ExternalModel)
	store := newModelInfoStore()
	p := &ModelProviderResolverPlugin{store: store}
	cs := framework.NewCycleState()
	req := framework.NewInferenceRequest()
	req.Body["model"] = "llama3-70b"

	err := p.ProcessRequest(context.Background(), cs, req)
	assert.NoError(t, err)

	_, provErr := framework.ReadCycleStateKey[string](cs, state.ProviderKey)
	assert.Error(t, provErr) // not found — internal models not in store
}

func TestProcessRequest_NoModel(t *testing.T) {
	store := newModelInfoStore()
	p := &ModelProviderResolverPlugin{store: store}
	cs := framework.NewCycleState()
	req := framework.NewInferenceRequest()
	// no "model" field in body

	err := p.ProcessRequest(context.Background(), cs, req)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "model")
}

func TestProcessRequest_NilRequest(t *testing.T) {
	store := newModelInfoStore()
	p := &ModelProviderResolverPlugin{store: store}
	cs := framework.NewCycleState()

	err := p.ProcessRequest(context.Background(), cs, nil)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "non-nil")
}

func TestProcessRequest_NoCredentialRef(t *testing.T) {
	store := newModelInfoStore()
	store.setModelInfo("gpt-4o", ModelInfo{
		provider: provider.OpenAI,
		// no credential ref
	}, types.NamespacedName{Name: "gpt-4o", Namespace: "llm"})

	p := &ModelProviderResolverPlugin{store: store}
	cs := framework.NewCycleState()
	req := framework.NewInferenceRequest()
	req.Body["model"] = "gpt-4o"

	err := p.ProcessRequest(context.Background(), cs, req)
	require.NoError(t, err)

	actualProvider, _ := framework.ReadCycleStateKey[string](cs, state.ProviderKey)
	assert.Equal(t, provider.OpenAI, actualProvider)

	credName, credErr := framework.ReadCycleStateKey[string](cs, "credential-ref-name")
	assert.NoError(t, credErr)
	assert.Equal(t, "", credName)
}

func TestModelStore_SetAndGet(t *testing.T) {
	store := newModelInfoStore()
	key := types.NamespacedName{Name: "test", Namespace: "ns"}

	store.setModelInfo("model-a", ModelInfo{provider: provider.Anthropic}, key)

	info, found := store.getModelInfo("model-a")
	assert.True(t, found)
	assert.Equal(t, provider.Anthropic, info.provider)
}

func TestModelStore_DeleteByResource(t *testing.T) {
	store := newModelInfoStore()
	key := types.NamespacedName{Name: "test", Namespace: "ns"}

	store.setModelInfo("model-a", ModelInfo{provider: provider.Anthropic}, key)
	store.deleteByResource(key)

	_, found := store.getModelInfo("model-a")
	assert.False(t, found)
}

func TestModelStore_DeleteNonExistent(t *testing.T) {
	store := newModelInfoStore()
	// should not panic
	store.deleteByResource(types.NamespacedName{Name: "nonexistent", Namespace: "ns"})
}
