package openai

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTranslateRequest_BasicChat(t *testing.T) {
	body := map[string]any{
		"model":    "gpt-4o-mini",
		"messages": []any{map[string]any{"role": "user", "content": "Hi"}},
	}

	translatedBody, headers, headersToRemove, err := NewOpenAITranslator().TranslateRequest(body)
	require.NoError(t, err)
	assert.Nil(t, translatedBody)
	assert.Equal(t, "/v1/chat/completions", headers[":path"])
	assert.Nil(t, headersToRemove)
}

func TestTranslateRequest_MissingModel(t *testing.T) {
	body := map[string]any{
		"messages": []any{map[string]any{"role": "user", "content": "Hi"}},
	}

	_, _, _, err := NewOpenAITranslator().TranslateRequest(body)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "model")
}

func TestTranslateRequest_EmptyMessages(t *testing.T) {
	body := map[string]any{
		"model":    "gpt-4o-mini",
		"messages": []any{},
	}

	_, _, _, err := NewOpenAITranslator().TranslateRequest(body)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "messages")
	assert.Contains(t, err.Error(), "BadRequest")
}

func TestTranslateRequest_MissingMessages(t *testing.T) {
	body := map[string]any{
		"model": "gpt-4o-mini",
	}

	_, _, _, err := NewOpenAITranslator().TranslateRequest(body)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "messages")
}

func TestTranslateResponse_Noop(t *testing.T) {
	body := map[string]any{"choices": []any{}}

	translatedBody, err := NewOpenAITranslator().TranslateResponse(body, "gpt-4o-mini")
	require.NoError(t, err)
	assert.Nil(t, translatedBody)
}
