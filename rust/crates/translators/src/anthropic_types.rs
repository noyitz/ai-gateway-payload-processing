use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct OpenAiRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    pub max_tokens: Option<i64>,
    pub max_completion_tokens: Option<i64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub stop: Option<StopValue>,
    pub tools: Option<Vec<OpenAiTool>>,
    pub tool_choice: Option<Value>,
    pub stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: Option<Content>,
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum StopValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub tool_type: Option<String>,
    pub function: Option<OpenAiFunction>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    pub function: OpenAiToolCallFunction,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiToolCallFunction {
    pub name: String,
    pub arguments: ArgumentsValue,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ArgumentsValue {
    String(String),
    Object(Value),
}

// --- Anthropic types (for building the translated request) ---

#[derive(Debug, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Blocks(Vec<Value>),
}

#[derive(Debug, Serialize)]
pub struct AnthropicTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
}

// --- Anthropic response types (for parsing provider response) ---

#[derive(Debug, Deserialize)]
pub struct AnthropicResponse {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub response_type: Option<String>,
    pub model: Option<String>,
    pub content: Option<Vec<AnthropicContentBlock>>,
    pub stop_reason: Option<String>,
    pub usage: Option<AnthropicUsage>,
    pub error: Option<AnthropicError>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub input: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicError {
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    pub message: Option<String>,
}
