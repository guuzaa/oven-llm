//! `oven-llm` Router 路由层示例：按模型 ID 自动派发 provider。
//!
//! 演示：
//! - 用 `ProviderBuilder`（显式 base_url）构造 DeepSeek / Zhipu 两个 provider
//! - 注册进 `Router`，并用前缀规则显式指定归属
//! - 通过 `router.complete` / `router.stream` 自动派发
//! - 未注册模型返回 `RouterError::UnknownModel`
//! - 用 `ModelRegistry` 维护模型目录并注入 provider，展示 Router 的
//!   "模型目录扫描"派发
//!
//! 未设置 API key 时使用占位 key，网络调用会失败并打印错误而不会 panic，
//! 因此没有真实 API key/网络也能演示完整的调用流程。

use std::io::{self, Write};
use std::time::Duration;

use futures::StreamExt;
use oven_llm::{
    Delta, ModelCapabilities, ModelId, ModelInfo, ModelRegistry, Pricing, ProviderBuilder,
    ProviderName, Request, Router, RouterError, StreamEvent, ThinkingMode,
};

fn api_key(env: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| "sk-placeholder".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. ModelRegistry 玩法：维护模型目录并注入 provider，让 Router 的
    //    "模型目录扫描"回退也能自动派发（无需 alias / route 规则）。
    let mut registry = ModelRegistry::from_models(vec![ModelInfo {
        id: "deepseek-v4-flash".to_string(),
        provider: ProviderName::DeepSeek,
        context_window: 1_000_000,
        max_output_tokens: 384_000,
        capabilities: ModelCapabilities {
            supports_tools: true,
            supports_streaming: true,
            supports_system_prompt: true,
            ..Default::default()
        },
        pricing: Some(Pricing {
            input_per_million: 0.14,
            output_per_million: 0.28,
        }),
    }]);

    // 也可以只用 id / provider 快速登记（`ModelInfo::minimal`）。
    registry.register(ModelInfo::minimal(
        "deepseek-v4-pro",
        ProviderName::DeepSeek,
    ));

    // 维护与查询：register 按 id 覆盖即"更新"，unregister 移除，
    // search / list_by_provider 检索。
    println!(
        "registry: search 'deepseek-v4-' -> {} model(s)",
        registry.search("deepseek-v4-").len()
    );
    if let Some(removed) = registry.unregister("deepseek-v4-pro") {
        println!("registry: unregistered {}", removed.id);
    }
    println!(
        "registry: after unregister, DeepSeek -> {} model(s)",
        registry.list_by_provider(&ProviderName::DeepSeek).len()
    );

    // 2. 用统一入口构造两个 provider。
    let deepseek = ProviderBuilder::completions()
        .provider_name(ProviderName::DeepSeek)
        .api_key(api_key("DEEPSEEK_API_KEY"))
        .base_url("https://api.deepseek.com")
        .model_registry(registry)
        .build()?;

    println!(
        "known_models -> {}",
        deepseek
            .known_models()
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let zhipu = ProviderBuilder::completions()
        .provider_name(ProviderName::Zhipu)
        .api_key(api_key("ZHIPU_API_KEY"))
        .base_url("https://open.bigmodel.cn/api/paas/v4")
        .build()?;

    // 3. 注册进 Router，并显式添加前缀规则（也可以依赖用户注入的模型目录自动派发）。
    let mut router = Router::new();
    router
        .register(deepseek)
        .register(zhipu)
        .alias("deepseek-v4-flash", &ProviderName::DeepSeek)
        .route("glm-", &ProviderName::Zhipu);

    // 4. 展示派发解析与未命中错误。
    for id in ["deepseek-v4-flash", "glm-5.2", "unknown-model"] {
        match router.provider(&ModelId::from(id)) {
            Ok(provider) => println!("{id} -> {}", provider.provider_name()),
            Err(RouterError::UnknownModel(model)) => println!("{id} -> unknown: {model}"),
            Err(err) => println!("{id} -> error: {err}"),
        }
    }

    // 5. 非流式调用（无真实 key 时会打印错误并继续）。
    let request = Request::builder()
        .model("deepseek-v4-flash")
        .prompt("用一句话介绍 Rust 语言。")
        .temperature(0.01)
        .thinking(ThinkingMode::Disabled)
        .build()?;
    match router.complete(&request).await {
        Ok(response) => println!("complete() -> {}", response.text()),
        Err(err) => eprintln!("complete() failed (expected without a real API key): {err}"),
    }

    // 6. 流式调用。
    tokio::time::sleep(Duration::from_secs(1)).await;
    match router.stream(&request).await {
        Ok(mut stream) => {
            print!("stream(): ");
            while let Some(event) = stream.next().await {
                match event {
                    Ok(StreamEvent::ContentBlockDelta {
                        delta: Delta::TextDelta { text },
                        ..
                    }) => {
                        print!("{text}");
                        io::stdout().flush()?;
                    }
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("\nstream() error (expected without a real API key): {err}");
                        break;
                    }
                }
            }
            println!();
        }
        Err(err) => eprintln!("stream() failed to start (expected without a real API key): {err}"),
    }

    Ok(())
}
