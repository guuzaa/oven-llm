//! `oven-llm` Router 路由层示例：按模型 ID 自动派发 provider。
//!
//! 演示：
//! - 用 `ProviderBuilder` 构造 DeepSeek / Zhipu 两个 provider
//! - 注册进 `Router`，并用前缀规则显式指定归属
//! - 通过 `router.complete` / `router.stream` 自动派发
//! - 未注册模型返回 `RouterError::UnknownModel`
//!
//! 未设置 API key 时使用占位 key，网络调用会失败并打印错误而不会 panic，
//! 因此没有真实 API key/网络也能演示完整的调用流程。

use std::io::{self, Write};
use std::time::Duration;

use futures::StreamExt;
use oven_llm::{
    Delta, Message, ModelId, ProviderBuilder, ProviderKind, ProviderName, Request, Router,
    RouterError, StreamEvent, ThinkingMode,
};

fn api_key(env: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| "sk-placeholder".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 用统一入口构造两个 provider。
    let deepseek = ProviderBuilder::new(ProviderKind::Completions)
        .provider_name(ProviderName::DeepSeek)
        .api_key(api_key("DEEPSEEK_API_KEY"))
        .build()?;
    let zhipu = ProviderBuilder::new(ProviderKind::Completions)
        .provider_name(ProviderName::Zhipu)
        .api_key(api_key("ZHIPU_API_KEY"))
        .build()?;

    // 2. 注册进 Router，并显式添加前缀规则（也可以依赖静态目录自动派发）。
    let mut router = Router::new();
    router
        .register(deepseek)
        .register(zhipu)
        .alias("deepseek-v4-flash", &ProviderName::DeepSeek)
        .route("glm-", &ProviderName::Zhipu);

    // 3. 展示派发解析与未命中错误。
    for id in ["deepseek-v4-flash", "glm-5.2", "unknown-model"] {
        match router.provider(&ModelId::from(id)) {
            Ok(provider) => println!("{id} -> {}", provider.provider_name()),
            Err(RouterError::UnknownModel(model)) => println!("{id} -> unknown: {model}"),
            Err(err) => println!("{id} -> error: {err}"),
        }
    }

    // 4. 非流式调用（无真实 key 时会打印错误并继续）。
    let request = Request::builder()
        .model("deepseek-v4-flash")
        .message(Message::user_text("用一句话介绍 Rust 语言。"))
        .temperature(0.0)
        .thinking(ThinkingMode::Enabled)
        .build()?;
    match router.complete(&request).await {
        Ok(response) => println!("complete() -> {}", response.text()),
        Err(err) => eprintln!("complete() failed (expected without a real API key): {err}"),
    }

    // 5. 流式调用。
    std::thread::sleep(Duration::from_secs(1));
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
