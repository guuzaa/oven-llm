//! `ModelRegistry`：管理多个 `ModelInfo` 的注册表。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中 "ModelRegistry"
//! 一节，以及需求文档 Requirement 8。

use std::collections::HashMap;

use crate::domain::ModelId;

use super::model::ModelInfo;

/// 以 `ModelId` 为键管理已知模型的注册表，支持按 ID 精确查找、按 provider 列出、
/// 按前缀搜索（Requirements 8.1-8.5）。
#[derive(Debug, Clone, Default)]
pub struct ModelRegistry {
    models: HashMap<ModelId, ModelInfo>,
}

impl ModelRegistry {
    /// 创建一个空的注册表。
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// 注册一个 `ModelInfo`。若 `id` 已存在，则用新值覆盖旧值
    /// （Requirement 8.2）。
    pub fn register(&mut self, info: ModelInfo) {
        self.models.insert(ModelId::from(info.id.as_str()), info);
    }

    /// 批量注册，等价于对每个元素依次调用 `register`。
    pub fn register_all(&mut self, infos: impl IntoIterator<Item = ModelInfo>) {
        for info in infos {
            self.register(info);
        }
    }

    /// 按模型标识精确查找。支持传入 `&ModelId` 或 `&str`；未注册的 ID 返回
    /// `None`（Requirement 8.5）。
    pub fn get<Q>(&self, id: &Q) -> Option<&ModelInfo>
    where
        ModelId: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.models.get(id)
    }

    /// 返回 `provider` 字段与查询参数精确匹配的所有 `ModelInfo`
    /// （Requirement 8.3）。
    pub fn list_by_provider(&self, provider: &str) -> Vec<&ModelInfo> {
        self.models
            .values()
            .filter(|info| info.provider == provider)
            .collect()
    }

    /// 返回注册表中所有 `ModelInfo`。
    pub fn list_all(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }

    /// 返回 `id` 以给定前缀开头的所有 `ModelInfo`（Requirement 8.4）。
    pub fn search(&self, query: &str) -> Vec<&ModelInfo> {
        self.models
            .values()
            .filter(|info| info.id.starts_with(query))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::model::ModelCapabilities;

    fn model(id: &str, provider: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            provider: provider.to_string(),
            context_window: 1000,
            max_output_tokens: 100,
            capabilities: ModelCapabilities::default(),
            pricing: None,
        }
    }

    #[test]
    fn register_then_get_roundtrip() {
        let mut registry = ModelRegistry::new();
        let info = model("deepseek-chat", "deepseek");
        registry.register(info.clone());

        assert_eq!(registry.get("deepseek-chat"), Some(&info));
    }

    #[test]
    fn register_overrides_existing_id() {
        let mut registry = ModelRegistry::new();
        registry.register(model("deepseek-chat", "deepseek"));

        let mut updated = model("deepseek-chat", "deepseek");
        updated.context_window = 999_999;
        registry.register(updated.clone());

        assert_eq!(registry.get("deepseek-chat"), Some(&updated));
        assert_eq!(registry.list_all().len(), 1);
    }

    #[test]
    fn list_by_provider_filters_exact_matches() {
        let mut registry = ModelRegistry::new();
        registry.register_all([
            model("deepseek-chat", "deepseek"),
            model("deepseek-coder", "deepseek"),
            model("glm-4", "zhipu"),
        ]);

        let mut deepseek_ids: Vec<&str> = registry
            .list_by_provider("deepseek")
            .into_iter()
            .map(|info| info.id.as_str())
            .collect();
        deepseek_ids.sort();

        assert_eq!(deepseek_ids, vec!["deepseek-chat", "deepseek-coder"]);
        assert!(registry.list_by_provider("openai").is_empty());
    }

    #[test]
    fn search_matches_prefix_only() {
        let mut registry = ModelRegistry::new();
        registry.register_all([
            model("moonshot-v1-8k", "moonshot"),
            model("moonshot-v1-32k", "moonshot"),
            model("glm-4", "zhipu"),
        ]);

        let mut matches: Vec<&str> = registry
            .search("moonshot-v1")
            .into_iter()
            .map(|info| info.id.as_str())
            .collect();
        matches.sort();

        assert_eq!(matches, vec!["moonshot-v1-32k", "moonshot-v1-8k"]);
        assert!(registry.search("gpt").is_empty());
    }

    #[test]
    fn get_returns_none_for_unregistered_id() {
        let registry = ModelRegistry::new();
        assert_eq!(registry.get("does-not-exist"), None);
    }
}
