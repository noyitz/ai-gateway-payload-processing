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

package azureopenai

import (
	"fmt"
	"regexp"
)

const (
	// defaultAPIVersion is the default Azure OpenAI API version.
	// Reference: https://learn.microsoft.com/en-us/azure/ai-services/openai/reference
	defaultAPIVersion = "2024-10-21"

	// Azure OpenAI endpoint path template.
	// The deployment ID typically matches the deployed model name.
	// Reference: https://learn.microsoft.com/en-us/azure/ai-foundry/openai/reference#chat-completions
	azurePathTemplate = "/openai/deployments/%s/chat/completions?api-version=%s"
)

// deploymentIDPattern validates Azure deployment IDs to prevent path/query injection.
var deploymentIDPattern = regexp.MustCompile(`^[a-zA-Z0-9][a-zA-Z0-9._-]*$`)

// AzureOpenAIProvider translates between OpenAI Chat Completions format and
// Azure OpenAI Service format. Azure OpenAI uses the same request/response schema
// as OpenAI, so translation is limited to path rewriting and header adjustments.
type AzureOpenAIProvider struct {
	apiVersion string
}

func NewAzureOpenAIProvider() *AzureOpenAIProvider {
	return &AzureOpenAIProvider{apiVersion: defaultAPIVersion}
}

// TranslateRequest rewrites the path and headers for Azure OpenAI.
// The request body is not mutated since Azure OpenAI accepts the same schema as OpenAI.
// Azure ignores the model field in the body and uses the deployment ID from the URI path.
func (p *AzureOpenAIProvider) TranslateRequest(body map[string]any) (map[string]any, map[string]string, []string, error) {
	model, _ := body["model"].(string)
	if model == "" {
		return nil, nil, nil, fmt.Errorf("model field is required")
	}
	if !deploymentIDPattern.MatchString(model) {
		return nil, nil, nil, fmt.Errorf("model %q contains invalid characters for Azure deployment ID", model)
	}

	headers := map[string]string{
		":path":        fmt.Sprintf(azurePathTemplate, model, p.apiVersion),
		"content-type": "application/json",
	}

	// Azure uses "api-key" header instead of "Authorization: Bearer".
	// The api-key is expected to be set by the upstream infrastructure layer.
	headersToRemove := []string{"authorization"}

	// Return nil body — no mutation needed, Azure accepts the OpenAI request format as-is.
	return nil, headers, headersToRemove, nil
}

// TranslateResponse is a no-op since Azure OpenAI returns responses in OpenAI format.
func (p *AzureOpenAIProvider) TranslateResponse(body map[string]any, model string) (map[string]any, error) {
	return nil, nil
}
