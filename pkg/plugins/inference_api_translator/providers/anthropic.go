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

package providers

import (
	"fmt"
	"time"
)

const (
	anthropicAPIVersion  = "2023-06-01"
	anthropicPath        = "/v1/messages"
	defaultMaxTokens     = 4096
)

// AnthropicProvider translates between OpenAI Chat Completions format and
// Anthropic Messages API format. Works with generic map[string]any bodies.
type AnthropicProvider struct{}

func NewAnthropicProvider() *AnthropicProvider {
	return &AnthropicProvider{}
}

func (p *AnthropicProvider) Name() string {
	return "anthropic"
}

// TranslateRequest translates an OpenAI Chat Completions request body to Anthropic Messages API format.
func (p *AnthropicProvider) TranslateRequest(body map[string]any) (map[string]any, map[string]string, []string, error) {
	model, _ := body["model"].(string)
	if model == "" {
		return nil, nil, nil, fmt.Errorf("model field is required")
	}

	messages, err := extractMessages(body)
	if err != nil {
		return nil, nil, nil, fmt.Errorf("failed to extract messages: %w", err)
	}

	systemPrompt, anthropicMessages := separateSystemMessages(messages)

	if len(anthropicMessages) == 0 {
		return nil, nil, nil, fmt.Errorf("at least one non-system message is required")
	}

	maxTokens := resolveMaxTokens(body)

	translated := map[string]any{
		"model":      model,
		"messages":   anthropicMessages,
		"max_tokens": maxTokens,
	}

	if systemPrompt != "" {
		translated["system"] = systemPrompt
	}

	if temp, ok := getFloat(body, "temperature"); ok {
		translated["temperature"] = temp
	}
	if topP, ok := getFloat(body, "top_p"); ok {
		translated["top_p"] = topP
	}
	if stop := extractStopSequences(body); len(stop) > 0 {
		translated["stop_sequences"] = stop
	}

	headers := map[string]string{
		"anthropic-version": anthropicAPIVersion,
		"content-type":      "application/json",
		":path":             anthropicPath,
	}

	headersToRemove := []string{"authorization", "content-length"}

	return translated, headers, headersToRemove, nil
}

// TranslateResponse translates an Anthropic Messages API response to OpenAI Chat Completions format.
func (p *AnthropicProvider) TranslateResponse(body map[string]any, model string) (map[string]any, error) {
	content := extractAnthropicContent(body)
	finishReason := mapStopReason(body)
	usage := mapAnthropicUsage(body)

	id, _ := body["id"].(string)
	if model == "" {
		model, _ = body["model"].(string)
	}

	translated := map[string]any{
		"id":      id,
		"object":  "chat.completion",
		"created": time.Now().Unix(),
		"model":   model,
		"choices": []any{
			map[string]any{
				"index": 0,
				"message": map[string]any{
					"role":    "assistant",
					"content": content,
				},
				"finish_reason": finishReason,
			},
		},
		"usage": usage,
	}

	return translated, nil
}

// separateSystemMessages extracts the system prompt and returns non-system messages
// in Anthropic format (with role and content fields).
func separateSystemMessages(messages []map[string]any) (string, []map[string]any) {
	var systemPrompt string
	var anthropicMessages []map[string]any

	for _, msg := range messages {
		role, _ := msg["role"].(string)
		content := extractContentString(msg)

		switch role {
		case "system":
			systemPrompt = content
		case "user":
			anthropicMessages = append(anthropicMessages, map[string]any{
				"role":    "user",
				"content": content,
			})
		case "assistant":
			anthropicMessages = append(anthropicMessages, map[string]any{
				"role":    "assistant",
				"content": content,
			})
		}
	}

	return systemPrompt, anthropicMessages
}

// extractMessages extracts the messages array from the request body.
func extractMessages(body map[string]any) ([]map[string]any, error) {
	rawMessages, ok := body["messages"]
	if !ok {
		return nil, fmt.Errorf("messages field is required")
	}

	messagesSlice, ok := rawMessages.([]any)
	if !ok {
		return nil, fmt.Errorf("messages must be an array")
	}

	var messages []map[string]any
	for i, raw := range messagesSlice {
		msg, ok := raw.(map[string]any)
		if !ok {
			return nil, fmt.Errorf("message at index %d is not an object", i)
		}
		messages = append(messages, msg)
	}

	return messages, nil
}

// extractContentString extracts text content from a message, handling both
// string content and array-of-content-parts formats.
func extractContentString(msg map[string]any) string {
	content, ok := msg["content"]
	if !ok {
		return ""
	}

	// Simple string content
	if s, ok := content.(string); ok {
		return s
	}

	// Array of content parts (e.g., [{type: "text", text: "hello"}])
	if parts, ok := content.([]any); ok {
		var texts []string
		for _, part := range parts {
			if partMap, ok := part.(map[string]any); ok {
				if text, ok := partMap["text"].(string); ok {
					texts = append(texts, text)
				}
			}
		}
		if len(texts) > 0 {
			return joinStrings(texts, " ")
		}
	}

	return fmt.Sprintf("%v", content)
}

// resolveMaxTokens extracts max tokens from the request body, checking
// max_completion_tokens first, then max_tokens, defaulting to 4096.
func resolveMaxTokens(body map[string]any) int {
	if v, ok := getInt(body, "max_completion_tokens"); ok && v > 0 {
		return v
	}
	if v, ok := getInt(body, "max_tokens"); ok && v > 0 {
		return v
	}
	return defaultMaxTokens
}

// extractStopSequences extracts stop sequences from the request body,
// handling both string and array formats.
func extractStopSequences(body map[string]any) []string {
	stop, ok := body["stop"]
	if !ok {
		return nil
	}

	if s, ok := stop.(string); ok && s != "" {
		return []string{s}
	}

	if arr, ok := stop.([]any); ok {
		var sequences []string
		for _, v := range arr {
			if s, ok := v.(string); ok {
				sequences = append(sequences, s)
			}
		}
		return sequences
	}

	return nil
}

// extractAnthropicContent extracts text from Anthropic response content blocks.
func extractAnthropicContent(body map[string]any) string {
	contentBlocks, ok := body["content"].([]any)
	if !ok {
		return ""
	}

	var texts []string
	for _, block := range contentBlocks {
		if blockMap, ok := block.(map[string]any); ok {
			if blockType, _ := blockMap["type"].(string); blockType == "text" {
				if text, ok := blockMap["text"].(string); ok {
					texts = append(texts, text)
				}
			}
		}
	}

	return joinStrings(texts, "")
}

// mapStopReason maps Anthropic stop_reason to OpenAI finish_reason.
func mapStopReason(body map[string]any) string {
	reason, _ := body["stop_reason"].(string)
	switch reason {
	case "max_tokens":
		return "length"
	case "tool_use":
		return "tool_calls"
	default:
		return "stop"
	}
}

// mapAnthropicUsage maps Anthropic usage fields to OpenAI format.
func mapAnthropicUsage(body map[string]any) map[string]any {
	usage, ok := body["usage"].(map[string]any)
	if !ok {
		return map[string]any{
			"prompt_tokens":     0,
			"completion_tokens": 0,
			"total_tokens":      0,
		}
	}

	inputTokens := toInt(usage["input_tokens"])
	outputTokens := toInt(usage["output_tokens"])

	return map[string]any{
		"prompt_tokens":     inputTokens,
		"completion_tokens": outputTokens,
		"total_tokens":      inputTokens + outputTokens,
	}
}

// Helper functions for type-safe extraction from map[string]any

func getFloat(body map[string]any, key string) (float64, bool) {
	v, ok := body[key]
	if !ok {
		return 0, false
	}
	switch f := v.(type) {
	case float64:
		return f, true
	case int:
		return float64(f), true
	case int64:
		return float64(f), true
	default:
		return 0, false
	}
}

func getInt(body map[string]any, key string) (int, bool) {
	v, ok := body[key]
	if !ok {
		return 0, false
	}
	return toInt(v), true
}

func toInt(v any) int {
	switch n := v.(type) {
	case float64:
		return int(n)
	case int:
		return n
	case int64:
		return int(n)
	default:
		return 0
	}
}

func joinStrings(parts []string, sep string) string {
	if len(parts) == 0 {
		return ""
	}
	result := parts[0]
	for _, p := range parts[1:] {
		result += sep + p
	}
	return result
}
