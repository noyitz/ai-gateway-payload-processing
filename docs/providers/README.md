# Provider Guides

Configuration guides for each supported external model provider.

| Provider | Type | Translation | Auth | Guide |
|----------|------|------------|------|-------|
| [OpenAI](openai.md) | `openai` | Pass-through | `Authorization: Bearer` | [openai.md](openai.md) |
| [Anthropic](anthropic.md) | `anthropic` | OpenAI ↔ Messages API | `x-api-key` | [anthropic.md](anthropic.md) |
| [AWS Bedrock](bedrock-openai.md) | `bedrock-openai` | Pass-through (Mantle) | `Authorization: Bearer` | [bedrock-openai.md](bedrock-openai.md) |
| [Azure OpenAI](azure-openai.md) | `azure-openai` | Path rewrite + field stripping | `api-key` | [azure-openai.md](azure-openai.md) |
| [Vertex AI](vertex-openai.md) | `vertex-openai` | Path rewrite + field stripping | `Authorization: Bearer` (OAuth) | [vertex-openai.md](vertex-openai.md) |
