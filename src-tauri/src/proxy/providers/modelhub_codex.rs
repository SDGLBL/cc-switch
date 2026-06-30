use crate::provider::Provider;
use serde_json::{json, Value as JsonValue};

const MODELHUB_PROVIDER_TYPE: &str = "modelhub_codex";
const MODELHUB_PRESET: &str = "modelhub";
const DEFAULT_MODELHUB_ROOT: &str = "https://aidp.bytedance.net/api/modelhub/online";

pub(crate) fn overlay_provider_for_request(
    provider: &Provider,
    body: &JsonValue,
) -> Option<Provider> {
    if !is_enabled(provider) {
        return None;
    }

    let model = request_model(body).unwrap_or("");
    let root = modelhub_root_url(provider);

    let is_gpt = model.to_ascii_lowercase().contains("gpt");
    let mut overlay = provider.clone();
    let mut meta = overlay.meta.unwrap_or_default();
    meta.is_full_url = Some(true);
    meta.api_format = Some(if is_gpt {
        "openai_responses".to_string()
    } else {
        "openai_chat".to_string()
    });
    overlay.meta = Some(meta);

    let mut settings = overlay
        .settings_config
        .as_object()
        .cloned()
        .unwrap_or_default();
    settings.insert(
        "base_url".to_string(),
        JsonValue::String(if is_gpt {
            format!("{root}/responses")
        } else {
            format!("{root}/v2/crawl")
        }),
    );

    if !model.is_empty() {
        settings.insert("model".to_string(), JsonValue::String(model.to_string()));
        settings.insert(
            "modelCatalog".to_string(),
            json!({
                "models": [
                    { "model": model }
                ]
            }),
        );
    }

    overlay.settings_config = JsonValue::Object(settings);
    Some(overlay)
}

pub(crate) fn preserve_request_model(source: &JsonValue, target: &mut JsonValue) {
    if let Some(model) = request_model(source) {
        target["model"] = JsonValue::String(model.to_string());
    }
}

fn request_model(body: &JsonValue) -> Option<&str> {
    body.get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|model| !model.is_empty())
}

fn is_enabled(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .is_some_and(|provider_type| provider_type == MODELHUB_PROVIDER_TYPE)
        || string_setting(provider, &["providerType"])
            .is_some_and(|provider_type| provider_type == MODELHUB_PROVIDER_TYPE)
        || string_setting(
            provider,
            &[
                "codexUpstreamPreset",
                "codex_upstream_preset",
                "upstreamPreset",
                "upstream_preset",
            ],
        )
        .is_some_and(|preset| preset.eq_ignore_ascii_case(MODELHUB_PRESET))
}

fn modelhub_root_url(provider: &Provider) -> String {
    string_setting(
        provider,
        &[
            "modelhubRootUrl",
            "modelhub_root_url",
            "modelhubBaseUrl",
            "modelhub_base_url",
        ],
    )
    .unwrap_or(DEFAULT_MODELHUB_ROOT)
    .trim_end_matches('/')
    .to_string()
}

fn string_setting<'a>(provider: &'a Provider, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| {
            provider
                .settings_config
                .get(*name)
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderMeta;
    use serde_json::json;

    fn create_provider(settings_config: JsonValue, provider_type: Option<&str>) -> Provider {
        Provider {
            id: "modelhub".to_string(),
            name: "ModelHub Codex".to_string(),
            settings_config,
            website_url: None,
            category: Some("codex".to_string()),
            created_at: None,
            sort_index: None,
            notes: None,
            meta: provider_type.map(|value| ProviderMeta {
                provider_type: Some(value.to_string()),
                ..Default::default()
            }),
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn modelhub_provider_type_enables_route() {
        let provider = create_provider(json!({}), Some("modelhub_codex"));

        assert!(is_enabled(&provider));
    }

    #[test]
    fn modelhub_preset_setting_enables_route() {
        let provider = create_provider(
            json!({
                "codexUpstreamPreset": "modelhub"
            }),
            None,
        );

        assert!(is_enabled(&provider));
    }

    #[test]
    fn gpt_models_overlay_to_native_responses() {
        let provider = create_provider(json!({}), Some("modelhub_codex"));

        let overlay = overlay_provider_for_request(&provider, &json!({ "model": "gpt-5.5-codex" }))
            .expect("modelhub overlay");

        assert_eq!(
            overlay
                .settings_config
                .get("base_url")
                .and_then(|value| value.as_str()),
            Some("https://aidp.bytedance.net/api/modelhub/online/responses")
        );
        assert_eq!(
            overlay.meta.as_ref().and_then(|meta| meta.is_full_url),
            Some(true)
        );
        assert_eq!(
            overlay
                .meta
                .as_ref()
                .and_then(|meta| meta.api_format.as_deref()),
            Some("openai_responses")
        );
        assert_eq!(overlay.settings_config["model"], "gpt-5.5-codex");
        assert_eq!(
            overlay.settings_config["modelCatalog"]["models"][0]["model"],
            "gpt-5.5-codex"
        );
    }

    #[test]
    fn non_gpt_models_overlay_to_crawl_with_request_model_catalog() {
        let provider = create_provider(json!({}), Some("modelhub_codex"));

        let overlay = overlay_provider_for_request(&provider, &json!({ "model": "glm-5.2" }))
            .expect("modelhub overlay");

        assert_eq!(
            overlay
                .settings_config
                .get("base_url")
                .and_then(|value| value.as_str()),
            Some("https://aidp.bytedance.net/api/modelhub/online/v2/crawl")
        );
        assert_eq!(
            overlay.meta.as_ref().and_then(|meta| meta.is_full_url),
            Some(true)
        );
        assert_eq!(
            overlay
                .meta
                .as_ref()
                .and_then(|meta| meta.api_format.as_deref()),
            Some("openai_chat")
        );
        assert_eq!(
            overlay.settings_config["modelCatalog"]["models"][0]["model"],
            "glm-5.2"
        );
    }

    #[test]
    fn overlay_keeps_auth_settings_but_rebases_model_to_request() {
        let provider = create_provider(
            json!({
                "api_key": "test-key",
                "model": "configured-model"
            }),
            Some("modelhub_codex"),
        );

        let overlay = overlay_provider_for_request(&provider, &json!({ "model": "kimi-k2" }))
            .expect("modelhub overlay");

        assert_eq!(overlay.settings_config["api_key"], "test-key");
        assert_eq!(overlay.settings_config["model"], "kimi-k2");
        assert_eq!(
            overlay.settings_config["modelCatalog"]["models"][0]["model"],
            "kimi-k2"
        );
    }

    #[test]
    fn preserve_request_model_restores_original_body_model() {
        let source = json!({ "model": "glm-5.2" });
        let mut target = json!({ "model": "configured-model" });

        preserve_request_model(&source, &mut target);

        assert_eq!(target["model"], "glm-5.2");
    }

    #[test]
    fn custom_modelhub_root_is_supported() {
        let provider = create_provider(
            json!({
                "modelhubRootUrl": "https://example.test/modelhub/"
            }),
            Some("modelhub_codex"),
        );

        let overlay = overlay_provider_for_request(&provider, &json!({ "model": "kimi-k2" }))
            .expect("modelhub overlay");

        assert_eq!(
            overlay
                .settings_config
                .get("base_url")
                .and_then(|value| value.as_str()),
            Some("https://example.test/modelhub/v2/crawl")
        );
    }
}
