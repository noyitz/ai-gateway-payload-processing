package translator_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/api-translation/translator/anthropic"
	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/api-translation/translator/azure"
	"github.com/opendatahub-io/ai-gateway-payload-processing/pkg/plugins/api-translation/translator/openai"
)

func testdataDir() string {
	_, filename, _, _ := runtime.Caller(0)
	return filepath.Join(filepath.Dir(filename), "testdata")
}

func loadFixture(tb testing.TB, name string) map[string]any {
	data, err := os.ReadFile(filepath.Join(testdataDir(), name))
	if err != nil {
		tb.Fatalf("Failed to read fixture %s: %v", name, err)
	}
	var body map[string]any
	if err := json.Unmarshal(data, &body); err != nil {
		tb.Fatalf("Failed to parse fixture %s: %v", name, err)
	}
	return body
}

func loadFixtureBytes(tb testing.TB, name string) []byte {
	data, err := os.ReadFile(filepath.Join(testdataDir(), name))
	if err != nil {
		tb.Fatalf("Failed to read fixture %s: %v", name, err)
	}
	return data
}

// --- Anthropic request benchmarks ---

func BenchmarkAnthropicRequestBasic(b *testing.B) {
	translator := anthropic.NewAnthropicTranslator()
	body := loadFixture(b, "openai_basic_request.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _, _, err := translator.TranslateRequest(body)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkAnthropicRequestComplex(b *testing.B) {
	translator := anthropic.NewAnthropicTranslator()
	body := loadFixture(b, "openai_complex_request.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _, _, err := translator.TranslateRequest(body)
		if err != nil {
			b.Fatal(err)
		}
	}
}

// --- Anthropic response benchmarks ---

func BenchmarkAnthropicResponseText(b *testing.B) {
	translator := anthropic.NewAnthropicTranslator()
	body := loadFixture(b, "anthropic_response_text.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := translator.TranslateResponse(body, "claude-3-5-sonnet-20241022")
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkAnthropicResponseToolUse(b *testing.B) {
	translator := anthropic.NewAnthropicTranslator()
	body := loadFixture(b, "anthropic_response_tool_use.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := translator.TranslateResponse(body, "claude-3-5-sonnet-20241022")
		if err != nil {
			b.Fatal(err)
		}
	}
}

// --- OpenAI passthrough benchmark ---

func BenchmarkOpenAIPassthrough(b *testing.B) {
	translator := openai.NewOpenAITranslator()
	body := loadFixture(b, "openai_basic_request.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _, _, err := translator.TranslateRequest(body)
		if err != nil {
			b.Fatal(err)
		}
	}
}

// --- Azure response strip benchmark ---

func BenchmarkAzureResponseStrip(b *testing.B) {
	translator := azure.NewAzureOpenAITranslator()
	body := loadFixture(b, "azure_response_with_filters.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := translator.TranslateResponse(body, "gpt-4o")
		if err != nil {
			b.Fatal(err)
		}
	}
}

// --- JSON parse/serialize benchmark ---

func BenchmarkJSONParseSerializeComplex(b *testing.B) {
	raw := loadFixtureBytes(b, "openai_complex_request.json")

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		var body map[string]any
		if err := json.Unmarshal(raw, &body); err != nil {
			b.Fatal(err)
		}
		output, err := json.Marshal(body)
		if err != nil {
			b.Fatal(err)
		}
		_ = output
	}
}
