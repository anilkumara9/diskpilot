//! Assistant provider data table. Requests go directly to the provider host
//! over HTTPS with no redirects; one key per provider, stored through the
//! key-storage abstraction (Windows Credential Manager via keyring).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAI,
    OpenRouter,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub provider: Provider,
    pub label: &'static str,
    pub host: &'static str,
    pub api_base: &'static str,
    pub default_model: &'static str,
    pub key_prefix_hint: &'static str,
    pub key_service: &'static str,
}

pub fn provider_info(provider: Provider) -> ProviderInfo {
    match provider {
        Provider::Anthropic => ProviderInfo {
            provider,
            label: "Anthropic",
            host: "api.anthropic.com",
            api_base: "https://api.anthropic.com/v1",
            default_model: "claude-sonnet-4-5",
            key_prefix_hint: "sk-ant-",
            key_service: "diskpilot.anthropic",
        },
        Provider::OpenAI => ProviderInfo {
            provider,
            label: "OpenAI",
            host: "api.openai.com",
            api_base: "https://api.openai.com/v1",
            default_model: "gpt-4o-mini",
            key_prefix_hint: "sk-",
            key_service: "diskpilot.openai",
        },
        Provider::OpenRouter => ProviderInfo {
            provider,
            label: "OpenRouter",
            host: "openrouter.ai",
            api_base: "https://openrouter.ai/api/v1",
            default_model: "openai/gpt-4o-mini",
            key_prefix_hint: "sk-or-",
            key_service: "diskpilot.openrouter",
        },
    }
}

pub fn all_providers() -> Vec<ProviderInfo> {
    vec![
        provider_info(Provider::Anthropic),
        provider_info(Provider::OpenAI),
        provider_info(Provider::OpenRouter),
    ]
}

pub fn key_looks_valid(provider: Provider, key: &str) -> bool {
    let info = provider_info(provider);
    let k = key.trim();
    k.len() >= 16 && k.starts_with(info.key_prefix_hint)
}

/// Storage abstraction for API keys. The keyring-backed implementation talks
/// to the OS credential store; tests use the in-memory implementation.
pub trait KeyStore {
    fn save(&self, provider: Provider, key: &str) -> Result<(), String>;
    fn load(&self, provider: Provider) -> Result<Option<String>, String>;
    fn delete(&self, provider: Provider) -> Result<(), String>;
    fn has(&self, provider: Provider) -> bool {
        matches!(self.load(provider), Ok(Some(_)))
    }
}

#[derive(Debug, Default)]
pub struct MemoryKeyStore {
    keys: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl KeyStore for MemoryKeyStore {
    fn save(&self, provider: Provider, key: &str) -> Result<(), String> {
        self.keys
            .lock()
            .map_err(|_| "lock poisoned".to_string())?
            .insert(
                provider_info(provider).key_service.to_string(),
                key.to_string(),
            );
        Ok(())
    }
    fn load(&self, provider: Provider) -> Result<Option<String>, String> {
        Ok(self
            .keys
            .lock()
            .map_err(|_| "lock poisoned".to_string())?
            .get(provider_info(provider).key_service)
            .cloned())
    }
    fn delete(&self, provider: Provider) -> Result<(), String> {
        self.keys
            .lock()
            .map_err(|_| "lock poisoned".to_string())?
            .remove(provider_info(provider).key_service);
        Ok(())
    }
}

#[cfg(feature = "keyring-storage")]
pub struct KeyringKeyStore;

#[cfg(feature = "keyring-storage")]
impl KeyStore for KeyringKeyStore {
    fn save(&self, provider: Provider, key: &str) -> Result<(), String> {
        let info = provider_info(provider);
        let entry = keyring::Entry::new(info.key_service, "api-key").map_err(|e| e.to_string())?;
        entry.set_password(key).map_err(|e| e.to_string())
    }
    fn load(&self, provider: Provider) -> Result<Option<String>, String> {
        let info = provider_info(provider);
        let entry = keyring::Entry::new(info.key_service, "api-key").map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn delete(&self, provider: Provider) -> Result<(), String> {
        let info = provider_info(provider);
        let entry = keyring::Entry::new(info.key_service, "api-key").map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Default store for the running app: keyring when the feature is enabled,
/// otherwise an in-memory store (keys then last only for the session, which
/// the Settings UI states plainly).
pub fn default_key_store() -> Box<dyn KeyStore + Send + Sync> {
    #[cfg(feature = "keyring-storage")]
    {
        Box::new(KeyringKeyStore)
    }
    #[cfg(not(feature = "keyring-storage"))]
    {
        Box::new(MemoryKeyStore::default())
    }
}
