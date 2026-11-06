pub mod provider;
pub mod tools;

pub use provider::{KeyStore, Provider, ProviderInfo};
pub use tools::{execute_tool, tool_definitions, ToolContext, ToolDefinition};

use serde_json::{json, Value};
use std::collections::HashSet;

/// The most tool-use rounds one question may take before the
/// session gives up: a model that keeps asking for tools after
/// five rounds is looping, not reasoning.
pub const MAX_TOOL_ROUNDS: usize = 5;

/// The system prompt the HTTP client sends when the caller does
/// not supply its own.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are DiskPilot's built-in assistant. \
You explain the user's own disk scan results and cleanup recommendations using the \
read-only tools provided. You cannot delete, move, or change anything; you can only \
report what the tools return and point at recommendations the user can review in \
the Cleanup page. Be concise and concrete, and use tool data rather than guesses.";

/// One tool call an assistant message requested: the provider's
/// call id (results are matched back by it), the tool name, and
/// the arguments as a JSON value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// One conversation message. Beyond the text, an assistant message
/// carries the tool calls it requested (`tool_calls`), and a
/// `role == "tool"` message carries the `tool_call_id` (and
/// `tool_name`) of the call it answers — the ids are what both
/// wire formats match results to calls with, so they are part of
/// the history, not decoration.
#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<ToolCallInfo>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ClientEvent {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    Done,
}

/// Transport abstraction: a real HTTP implementation lives behind
/// the `assistant-http` feature; tests drive the loop with a
/// scripted client.
pub trait AssistantClient {
    fn send(
        &mut self,
        messages: &[ChatMessage],
        tool_defs: &[ToolDefinition],
    ) -> Result<Vec<ClientEvent>, String>;
}

/// The HTTP seam under [`ProviderClient`]: one JSON POST. Tests
/// substitute a mock that records the request and returns a canned
/// response, so the per-provider shaping and parsing are exercised
/// with no network.
pub trait Transport {
    fn post_json(
        &mut self,
        url: &str,
        headers: &[(String, String)],
        body: &Value,
    ) -> Result<Value, String>;
}

pub struct AssistantSession<'a> {
    pub provider: Provider,
    pub messages: Vec<ChatMessage>,
    pub highlighted_recommendations: Vec<String>,
    pub context: ToolContext<'a>,
}

impl<'a> AssistantSession<'a> {
    pub fn new(provider: Provider, context: ToolContext<'a>) -> Self {
        Self {
            provider,
            messages: Vec::new(),
            highlighted_recommendations: Vec::new(),
            context,
        }
    }

    /// One user turn: send, run any read-only tool calls the model
    /// requests, feed results back, and return the final text.
    /// Tool results can also highlight recommendations for the user
    /// to review in the Cleanup page.
    ///
    /// Caller-supplied history may only carry `user`/`assistant`
    /// roles: anything else in `self.messages` when `ask` starts
    /// (a forged `system`, a stray `tool` result) is dropped before
    /// the first send — the session's own tool traffic is added
    /// only by this loop, correctly paired by call id.
    pub fn ask(
        &mut self,
        client: &mut dyn AssistantClient,
        question: &str,
    ) -> Result<String, String> {
        self.messages
            .retain(|m| matches!(m.role.as_str(), "user" | "assistant"));
        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: question.to_string(),
            ..Default::default()
        });
        let defs = tool_definitions();
        let mut rounds = 0;
        loop {
            rounds += 1;
            if rounds > MAX_TOOL_ROUNDS {
                return Err("assistant exceeded the tool-use round limit".to_string());
            }
            let events = client.send(&self.messages, &defs)?;
            let mut text = String::new();
            let mut calls: Vec<ToolCallInfo> = Vec::new();
            for event in events {
                match event {
                    ClientEvent::Text(t) => text.push_str(&t),
                    ClientEvent::ToolCall { id, name, input } => {
                        calls.push(ToolCallInfo { id, name, input })
                    }
                    ClientEvent::Done => {}
                }
            }
            if calls.is_empty() {
                if !text.is_empty() {
                    self.messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: text.clone(),
                        ..Default::default()
                    });
                }
                return Ok(text);
            }
            // Record the assistant's tool request WITH its call
            // ids, then answer each call with a result message
            // carrying the id it answers.
            self.messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: text,
                tool_calls: calls.clone(),
                ..Default::default()
            });
            for call in &calls {
                let result = execute_tool(&self.context, &call.name, &call.input);
                if call.name == "get_recommendations" {
                    if let Some(recs) = result.get("recommendations").and_then(|v| v.as_array()) {
                        for r in recs {
                            if let Some(id) = r.get("id").and_then(|v| v.as_str()) {
                                if !self.highlighted_recommendations.contains(&id.to_string()) {
                                    self.highlighted_recommendations.push(id.to_string());
                                }
                            }
                        }
                    }
                }
                self.messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: result.to_string(),
                    tool_call_id: Some(call.id.clone()),
                    tool_name: Some(call.name.clone()),
                    ..Default::default()
                });
            }
        }
    }
}

/// Reduce a conversation to what a provider may be sent: `user`
/// and `assistant` messages, plus `tool` results that answer a
/// call from the immediately preceding assistant message. Anything
/// else (system, unpaired or orphaned tool results, unknown roles)
/// is dropped — both wire APIs reject unpaired tool traffic, and
/// caller history is only ever user/assistant by contract.
fn sanitize_history(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    let mut pending: HashSet<&str> = HashSet::new();
    for m in messages {
        match m.role.as_str() {
            "user" => {
                pending.clear();
                out.push(m.clone());
            }
            "assistant" => {
                pending = m.tool_calls.iter().map(|c| c.id.as_str()).collect();
                out.push(m.clone());
            }
            "tool" => {
                if let Some(id) = m.tool_call_id.as_deref() {
                    if pending.remove(id) {
                        out.push(m.clone());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The endpoint path for a provider: Anthropic's Messages API vs
/// the Chat Completions API OpenAI and OpenRouter both speak.
fn endpoint_path(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "/messages",
        Provider::OpenAI | Provider::OpenRouter => "/chat/completions",
    }
}

/// Anthropic `/messages` body: system as a top-level field, tools
/// in Anthropic's shape, assistant tool requests as `tool_use`
/// blocks, and each run of tool results grouped into ONE user
/// message of `tool_result` blocks (the shape the API requires).
pub fn build_anthropic_body(
    model: &str,
    system: &str,
    messages: &[ChatMessage],
    tool_defs: &[ToolDefinition],
) -> Value {
    let history = sanitize_history(messages);
    let mut wire: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < history.len() {
        let m = &history[i];
        match m.role.as_str() {
            "user" => {
                wire.push(json!({"role": "user", "content": m.content}));
                i += 1;
            }
            "assistant" => {
                if m.tool_calls.is_empty() {
                    wire.push(json!({"role": "assistant", "content": m.content}));
                } else {
                    let mut blocks: Vec<Value> = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(json!({"type": "text", "text": m.content}));
                    }
                    for call in &m.tool_calls {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.name,
                            "input": call.input,
                        }));
                    }
                    wire.push(json!({"role": "assistant", "content": blocks}));
                }
                i += 1;
            }
            "tool" => {
                let mut blocks: Vec<Value> = Vec::new();
                while i < history.len() && history[i].role == "tool" {
                    let t = &history[i];
                    blocks.push(json!({
                        "type": "tool_result",
                        "tool_use_id": t.tool_call_id.clone().unwrap_or_default(),
                        "content": t.content,
                    }));
                    i += 1;
                }
                wire.push(json!({"role": "user", "content": blocks}));
            }
            _ => i += 1,
        }
    }
    let mut body = json!({
        "model": model,
        "max_tokens": 1024,
        "messages": wire,
    });
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if !tool_defs.is_empty() {
        body["tools"] = json!(tool_defs
            .iter()
            .map(|d| json!({
                "name": d.name,
                "description": d.description,
                "input_schema": d.input_schema,
            }))
            .collect::<Vec<_>>());
    }
    body
}

/// OpenAI-style `/chat/completions` body (OpenAI and OpenRouter):
/// system as the first message, tools as function definitions,
/// assistant tool requests as `tool_calls` with JSON-string
/// arguments, results as `role: "tool"` messages keyed by
/// `tool_call_id`.
pub fn build_openai_body(
    model: &str,
    system: &str,
    messages: &[ChatMessage],
    tool_defs: &[ToolDefinition],
) -> Value {
    let history = sanitize_history(messages);
    let mut wire: Vec<Value> = Vec::new();
    if !system.is_empty() {
        wire.push(json!({"role": "system", "content": system}));
    }
    for m in &history {
        match m.role.as_str() {
            "user" => wire.push(json!({"role": "user", "content": m.content})),
            "assistant" => {
                let mut msg = json!({"role": "assistant"});
                if m.tool_calls.is_empty() {
                    msg["content"] = json!(m.content);
                } else {
                    msg["content"] = if m.content.is_empty() {
                        Value::Null
                    } else {
                        json!(m.content)
                    };
                    msg["tool_calls"] = json!(m
                        .tool_calls
                        .iter()
                        .map(|c| json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": c.input.to_string(),
                            },
                        }))
                        .collect::<Vec<_>>());
                }
                wire.push(msg);
            }
            "tool" => wire.push(json!({
                "role": "tool",
                "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                "content": m.content,
            })),
            _ => {}
        }
    }
    let mut body = json!({
        "model": model,
        "messages": wire,
    });
    if !tool_defs.is_empty() {
        body["tools"] = json!(tool_defs
            .iter()
            .map(|d| json!({
                "type": "function",
                "function": {
                    "name": d.name,
                    "description": d.description,
                    "parameters": d.input_schema,
                },
            }))
            .collect::<Vec<_>>());
    }
    body
}

/// The request body for a provider, shaped for its own API.
pub fn build_body(
    provider: Provider,
    model: &str,
    system: &str,
    messages: &[ChatMessage],
    tool_defs: &[ToolDefinition],
) -> Value {
    match provider {
        Provider::Anthropic => build_anthropic_body(model, system, messages, tool_defs),
        Provider::OpenAI | Provider::OpenRouter => {
            build_openai_body(model, system, messages, tool_defs)
        }
    }
}

/// Parse an Anthropic `/messages` response into session events:
/// text blocks become text, `tool_use` blocks become tool calls.
pub fn parse_anthropic_response(value: &Value) -> Vec<ClientEvent> {
    let mut events = Vec::new();
    if let Some(blocks) = value.get("content").and_then(|c| c.as_array()) {
        for block in blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        events.push(ClientEvent::Text(text.to_string()));
                    }
                }
                Some("tool_use") => {
                    events.push(ClientEvent::ToolCall {
                        id: block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        name: block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        input: block.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
                _ => {}
            }
        }
    }
    events.push(ClientEvent::Done);
    events
}

/// Parse an OpenAI-style `/chat/completions` response: the first
/// choice's message text plus its `tool_calls` (arguments are a
/// JSON string, parsed back into a value; an unparseable argument
/// string is passed through as a raw string rather than dropped).
pub fn parse_openai_response(value: &Value) -> Vec<ClientEvent> {
    let mut events = Vec::new();
    let message = value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"));
    if let Some(message) = message {
        if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                events.push(ClientEvent::Text(text.to_string()));
            }
        }
        if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
            for call in calls {
                let function = call.get("function");
                let raw_args = function
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("");
                let input = serde_json::from_str::<Value>(raw_args)
                    .unwrap_or_else(|_| json!({ "raw": raw_args }));
                events.push(ClientEvent::ToolCall {
                    id: call
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: function
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    input,
                });
            }
        }
    }
    events.push(ClientEvent::Done);
    events
}

/// Parse a provider response into session events.
pub fn parse_response(provider: Provider, value: &Value) -> Vec<ClientEvent> {
    match provider {
        Provider::Anthropic => parse_anthropic_response(value),
        Provider::OpenAI | Provider::OpenRouter => parse_openai_response(value),
    }
}

/// An [`AssistantClient`] over any [`Transport`]: shapes the
/// request for the configured provider, posts it, parses the
/// response. This is the one client implementation; the HTTP
/// client below is this over reqwest.
pub struct ProviderClient<T: Transport> {
    pub provider: Provider,
    pub api_key: String,
    pub model: String,
    pub system: String,
    pub transport: T,
}

impl<T: Transport> AssistantClient for ProviderClient<T> {
    fn send(
        &mut self,
        messages: &[ChatMessage],
        tool_defs: &[ToolDefinition],
    ) -> Result<Vec<ClientEvent>, String> {
        let info = provider::provider_info(self.provider);
        let body = build_body(self.provider, &self.model, &self.system, messages, tool_defs);
        let headers: Vec<(String, String)> = match self.provider {
            Provider::Anthropic => vec![
                ("x-api-key".to_string(), self.api_key.clone()),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            Provider::OpenAI | Provider::OpenRouter => vec![
                (
                    "authorization".to_string(),
                    format!("Bearer {}", self.api_key),
                ),
                ("content-type".to_string(), "application/json".to_string()),
            ],
        };
        let url = format!("{}{}", info.api_base, endpoint_path(self.provider));
        let value = self.transport.post_json(&url, &headers, &body)?;
        Ok(parse_response(self.provider, &value))
    }
}

/// Build the HTTPS request description for a provider. The HTTP call itself
/// is feature-gated; this pure function pins the endpoint, headers shape and
/// the no-redirect policy so it can be tested offline.
pub fn build_request(provider: Provider, model: &str, system: &str) -> Value {
    let info = provider::provider_info(provider);
    json!({
        "url": format!("{}{}", info.api_base, endpoint_path(provider)),
        "host": info.host,
        "httpsOnly": true,
        "followRedirects": false,
        "model": model,
        "system": system,
    })
}

#[cfg(feature = "assistant-http")]
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

#[cfg(feature = "assistant-http")]
impl ReqwestTransport {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client })
    }
}

#[cfg(feature = "assistant-http")]
impl Transport for ReqwestTransport {
    fn post_json(
        &mut self,
        url: &str,
        headers: &[(String, String)],
        body: &Value,
    ) -> Result<Value, String> {
        let mut req = self.client.post(url).json(body);
        for (name, value) in headers {
            req = req.header(name.as_str(), value.as_str());
        }
        let resp = req.send().map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "provider returned HTTP {}: {}",
                status.as_u16(),
                text.chars().take(300).collect::<String>()
            ));
        }
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
}

/// The blocking HTTP client: a [`ProviderClient`] over reqwest
/// (HTTPS only, redirects disabled — the standing transport
/// policy). Per-provider request shaping, tool calls included,
/// comes from the shared implementation.
#[cfg(feature = "assistant-http")]
pub struct HttpAssistantClient {
    pub provider: Provider,
    pub api_key: String,
    pub model: String,
    pub system: String,
}

#[cfg(feature = "assistant-http")]
impl AssistantClient for HttpAssistantClient {
    fn send(
        &mut self,
        messages: &[ChatMessage],
        tool_defs: &[ToolDefinition],
    ) -> Result<Vec<ClientEvent>, String> {
        let mut client = ProviderClient {
            provider: self.provider,
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            system: self.system.clone(),
            transport: ReqwestTransport::new()?,
        };
        client.send(messages, tool_defs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn user(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: text.to_string(),
            ..Default::default()
        }
    }

    fn tool_exchange() -> Vec<ChatMessage> {
        vec![
            user("what uses space?"),
            ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                tool_calls: vec![
                    ToolCallInfo {
                        id: "call_1".to_string(),
                        name: "get_category_totals".to_string(),
                        input: json!({}),
                    },
                    ToolCallInfo {
                        id: "call_2".to_string(),
                        name: "get_top_items".to_string(),
                        input: json!({"limit": 3}),
                    },
                ],
                ..Default::default()
            },
            ChatMessage {
                role: "tool".to_string(),
                content: "{\"totalBytes\": 10}".to_string(),
                tool_call_id: Some("call_1".to_string()),
                tool_name: Some("get_category_totals".to_string()),
                ..Default::default()
            },
            ChatMessage {
                role: "tool".to_string(),
                content: "{\"topFiles\": []}".to_string(),
                tool_call_id: Some("call_2".to_string()),
                tool_name: Some("get_top_items".to_string()),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn anthropic_body_uses_tool_use_and_grouped_tool_results() {
        let body = build_anthropic_body(
            "claude-sonnet-4-5",
            "SYS",
            &tool_exchange(),
            &tool_definitions(),
        );
        assert_eq!(body["system"], "SYS");
        assert_eq!(body["tools"][0]["name"], "get_category_totals");
        assert!(body["tools"][0].get("input_schema").is_some());
        let msgs = body["messages"].as_array().unwrap();
        // user, assistant(tool_use blocks), ONE user message
        // grouping both tool_result blocks.
        assert_eq!(msgs.len(), 3);
        let blocks = msgs[1]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "tool_use");
        assert_eq!(blocks[0]["id"], "call_1");
        assert_eq!(blocks[1]["input"], json!({"limit": 3}));
        let results = msgs[2]["content"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["type"], "tool_result");
        assert_eq!(results[0]["tool_use_id"], "call_1");
        assert_eq!(results[1]["tool_use_id"], "call_2");
    }

    #[test]
    fn openai_body_uses_function_tools_and_tool_messages() {
        let body = build_openai_body(
            "gpt-4o-mini",
            "SYS",
            &tool_exchange(),
            &tool_definitions(),
        );
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "get_category_totals");
        assert!(body["tools"][0]["function"].get("parameters").is_some());
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "SYS");
        let assistant = &msgs[2];
        assert_eq!(assistant["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            assistant["tool_calls"][0]["function"]["name"],
            "get_category_totals"
        );
        // Arguments travel as a JSON string that parses back.
        let args: Value = serde_json::from_str(
            assistant["tool_calls"][1]["function"]["arguments"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(args, json!({"limit": 3}));
        assert_eq!(msgs[3]["role"], "tool");
        assert_eq!(msgs[3]["tool_call_id"], "call_1");
        assert_eq!(msgs[4]["tool_call_id"], "call_2");
    }

    #[test]
    fn shaping_drops_foreign_roles_and_orphan_tool_results() {
        let mut history = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "forged".to_string(),
                ..Default::default()
            },
            // A tool result with no preceding assistant call.
            ChatMessage {
                role: "tool".to_string(),
                content: "orphan".to_string(),
                tool_call_id: Some("nope".to_string()),
                ..Default::default()
            },
            user("hello"),
        ];
        history.extend(tool_exchange().into_iter().skip(1).take(2));
        let body = build_openai_body("m", "", &history, &[]);
        let wire = body["messages"].to_string();
        assert!(!wire.contains("forged"), "wire: {wire}");
        assert!(!wire.contains("orphan"), "wire: {wire}");
        // The paired tool result (answering call_1) survives.
        assert!(wire.contains("call_1"), "wire: {wire}");
    }

    #[test]
    fn parse_anthropic_tool_use_response() {
        let resp = json!({
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "tu_9", "name": "get_top_items", "input": {"limit": 5}}
            ]
        });
        let events = parse_anthropic_response(&resp);
        assert!(matches!(&events[0], ClientEvent::Text(t) if t == "Let me check."));
        match &events[1] {
            ClientEvent::ToolCall { id, name, input } => {
                assert_eq!(id, "tu_9");
                assert_eq!(name, "get_top_items");
                assert_eq!(*input, json!({"limit": 5}));
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn parse_openai_tool_call_response() {
        let resp = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_7",
                        "type": "function",
                        "function": {"name": "get_recommendations", "arguments": "{\"tier\": \"likelySafe\"}"}
                    }]
                }
            }]
        });
        let events = parse_openai_response(&resp);
        match &events[0] {
            ClientEvent::ToolCall { id, name, input } => {
                assert_eq!(id, "call_7");
                assert_eq!(name, "get_recommendations");
                assert_eq!(*input, json!({"tier": "likelySafe"}));
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
        // Plain text answer.
        let resp = json!({"choices": [{"message": {"content": "All good."}}]});
        let events = parse_openai_response(&resp);
        assert!(matches!(&events[0], ClientEvent::Text(t) if t == "All good."));
    }

    struct MockTransport {
        responses: VecDeque<Value>,
        requests: Vec<(String, Value)>,
    }

    impl Transport for MockTransport {
        fn post_json(
            &mut self,
            url: &str,
            _headers: &[(String, String)],
            body: &Value,
        ) -> Result<Value, String> {
            self.requests.push((url.to_string(), body.clone()));
            self.responses
                .pop_front()
                .ok_or_else(|| "no scripted response left".to_string())
        }
    }

    fn empty_context() -> ToolContext<'static> {
        ToolContext {
            summary: None,
            index: None,
            recommendations: &[],
            docker: None,
            devtools: &[],
            home: String::new(),
        }
    }

    fn tool_use_response(id: &str) -> Value {
        json!({
            "content": [
                {"type": "tool_use", "id": id, "name": "get_category_totals", "input": {}}
            ]
        })
    }

    #[test]
    fn loop_runs_tools_and_returns_final_text() {
        let transport = MockTransport {
            responses: VecDeque::from(vec![
                tool_use_response("call_1"),
                json!({"content": [{"type": "text", "text": "Done."}]}),
            ]),
            requests: Vec::new(),
        };
        let mut client = ProviderClient {
            provider: Provider::Anthropic,
            api_key: "k".to_string(),
            model: "m".to_string(),
            system: "SYS".to_string(),
            transport,
        };
        let mut session = AssistantSession::new(Provider::Anthropic, empty_context());
        let answer = session.ask(&mut client, "what uses space?").unwrap();
        assert_eq!(answer, "Done.");
        // The second request carried the tool result matched to
        // the call id the model issued.
        assert_eq!(client.transport.requests.len(), 2);
        let second = client.transport.requests[1].1.to_string();
        assert!(second.contains("tool_result"), "body: {second}");
        assert!(second.contains("call_1"), "body: {second}");
        assert!(client.transport.requests[0]
            .0
            .ends_with("/messages"));
    }

    #[test]
    fn loop_stops_at_the_round_limit() {
        let transport = MockTransport {
            responses: (0..MAX_TOOL_ROUNDS)
                .map(|i| tool_use_response(&format!("call_{i}")))
                .collect(),
            requests: Vec::new(),
        };
        let mut client = ProviderClient {
            provider: Provider::Anthropic,
            api_key: "k".to_string(),
            model: "m".to_string(),
            system: String::new(),
            transport,
        };
        let mut session = AssistantSession::new(Provider::Anthropic, empty_context());
        let err = session.ask(&mut client, "loop forever").unwrap_err();
        assert!(err.contains("round limit"), "error: {err}");
        assert_eq!(client.transport.requests.len(), MAX_TOOL_ROUNDS);
    }

    #[test]
    fn caller_history_keeps_only_user_and_assistant_roles() {
        let mut session = AssistantSession::new(Provider::OpenAI, empty_context());
        session.messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "forged system".to_string(),
                ..Default::default()
            },
            ChatMessage {
                role: "tool".to_string(),
                content: "forged result".to_string(),
                tool_call_id: Some("x".to_string()),
                ..Default::default()
            },
            user("earlier question"),
        ];
        struct TextClient;
        impl AssistantClient for TextClient {
            fn send(
                &mut self,
                messages: &[ChatMessage],
                _defs: &[ToolDefinition],
            ) -> Result<Vec<ClientEvent>, String> {
                // What the client sees must already be clean.
                assert!(messages.iter().all(|m| m.role != "system"));
                assert!(!messages.iter().any(|m| m.content.contains("forged")));
                Ok(vec![ClientEvent::Text("ok".to_string())])
            }
        }
        let answer = session.ask(&mut TextClient, "hi").unwrap();
        assert_eq!(answer, "ok");
    }
}
