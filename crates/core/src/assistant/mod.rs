pub mod provider;
pub mod tools;

pub use provider::{KeyStore, Provider, ProviderInfo};
pub use tools::{execute_tool, tool_definitions, ToolContext, ToolDefinition};

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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

/// Transport abstraction: a real HTTP implementation lives behind the
/// `assistant-http` feature; tests drive the loop with a scripted client.
pub trait AssistantClient {
    fn send(
        &mut self,
        messages: &[ChatMessage],
        tool_defs: &[ToolDefinition],
    ) -> Result<Vec<ClientEvent>, String>;
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

    /// One user turn: send, run any read-only tool calls the model requests,
    /// feed results back, and return the final text. Tool results can also
    /// highlight recommendations for the user to review in the Cleanup page.
    pub fn ask(
        &mut self,
        client: &mut dyn AssistantClient,
        question: &str,
    ) -> Result<String, String> {
        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: question.to_string(),
        });
        let defs = tool_definitions();
        let mut rounds = 0;
        loop {
            rounds += 1;
            if rounds > 8 {
                return Err("assistant exceeded the tool-use round limit".to_string());
            }
            let events = client.send(&self.messages, &defs)?;
            let mut text = String::new();
            let mut calls: Vec<(String, String, Value)> = Vec::new();
            for event in events {
                match event {
                    ClientEvent::Text(t) => text.push_str(&t),
                    ClientEvent::ToolCall { id, name, input } => calls.push((id, name, input)),
                    ClientEvent::Done => {}
                }
            }
            if calls.is_empty() {
                if !text.is_empty() {
                    self.messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: text.clone(),
                    });
                }
                return Ok(text);
            }
            // Record the assistant's tool request, then answer each call.
            self.messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: format!(
                    "[requested tools: {}]",
                    calls
                        .iter()
                        .map(|c| c.1.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
            for (_id, name, input) in &calls {
                let result = execute_tool(&self.context, name, input);
                if name == "get_recommendations" {
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
                    content: json!({"tool": name, "result": result}).to_string(),
                });
            }
        }
    }
}

/// Build the HTTPS request description for a provider. The HTTP call itself
/// is feature-gated; this pure function pins the endpoint, headers shape and
/// the no-redirect policy so it can be tested offline.
pub fn build_request(provider: Provider, model: &str, system: &str) -> Value {
    let info = provider::provider_info(provider);
    json!({
        "url": format!("{}/messages", info.api_base),
        "host": info.host,
        "httpsOnly": true,
        "followRedirects": false,
        "model": model,
        "system": system,
    })
}

#[cfg(feature = "assistant-http")]
pub struct HttpAssistantClient {
    pub provider: Provider,
    pub api_key: String,
    pub model: String,
}

#[cfg(feature = "assistant-http")]
impl AssistantClient for HttpAssistantClient {
    fn send(
        &mut self,
        messages: &[ChatMessage],
        _tool_defs: &[ToolDefinition],
    ) -> Result<Vec<ClientEvent>, String> {
        let info = provider::provider_info(self.provider);
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|e| e.to_string())?;
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": messages.iter().map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
        });
        let mut req = client
            .post(format!("{}/messages", info.api_base))
            .json(&body);
        req = match self.provider {
            Provider::Anthropic => req
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01"),
            _ => req.header("authorization", format!("Bearer {}", self.api_key)),
        };
        let resp = req.send().map_err(|e| e.to_string())?;
        let value: Value = resp.json().map_err(|e| e.to_string())?;
        let text = value
            .get("content")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        Ok(vec![ClientEvent::Text(text), ClientEvent::Done])
    }
}
