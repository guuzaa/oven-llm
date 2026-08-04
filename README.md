# oven-llm

[![crates.io](https://img.shields.io/crates/v/oven-llm.svg)](https://crates.io/crates/oven-llm)
[![docs.rs](https://docs.rs/oven-llm/badge.svg)](https://docs.rs/oven-llm)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust library for calling LLM providers through one unified, async runtime-agnostic API.

`oven-llm` exposes a single `Provider` trait, a provider-agnostic domain model (`Request` /
`Response` / `StreamEvent` / `Message` / `Tool`), and two wire protocol implementations — the
OpenAI Chat Completions compatible protocol and the OpenAI Responses API — with built-in presets
for DeepSeek, Moonshot (Kimi), Zhipu GLM, xAI Grok, and OpenAI itself. Switch vendors by
changing one constructor call; application code never touches wire formats.

> **Status: not production-ready.** This crate is under active development. The public API is
> still evolving and may change without notice, and there are no stability or correctness
> guarantees yet. Please evaluate it before using it in production workloads.

## Features

- **Unified `Provider` trait** — `complete`, `stream`, and `list_models` behind one trait. Any
  provider can be used as `&dyn Provider`, so harness/application code is fully decoupled from
  vendor wire formats.
- **Provider-agnostic domain model** — `Request` (builder API), `Message` with `ContentBlock`
  (text / thinking / image / tool use / tool result), `Response` with helpers like `text()`,
  `thinking()` and `tool_uses()`, all serializable and round-trippable.
- **Two OpenAI-family protocols**:
  - `CompletionsProvider` — OpenAI Chat Completions compatible protocol (`POST /chat/completions`),
    spoken by OpenAI, DeepSeek, Moonshot (Kimi), Zhipu GLM and many gateways.
  - `ResponsesProvider` — OpenAI Responses API (`POST /responses`), with DeepSeek and xAI Grok
    presets.
- **Streaming everywhere** — SSE responses are normalized into a unified Anthropic-style
  `StreamEvent` stream (`MessageStart` / `ContentBlockDelta` / `MessageStop` …), and
  `StreamCollector` rebuilds a complete `Response` from the event stream.
- **Tool / function calling** — JSON-Schema tool definitions, `ToolChoice` policies (auto / any /
  none / forced tool), streamed `InputJsonDelta` tool arguments, and a natural multi-turn agent
  loop pattern.
- **Thinking & reasoning** — `ThinkingMode`, `ReasoningEffort`, `ContentBlock::Thinking`,
  `Delta::ThinkingDelta`, and `Usage::reasoning_tokens` across both protocols.
- **Model metadata, validation & discovery** — static model catalogs carrying context window, max
  output tokens, capabilities and pricing; `validate_request` / `estimate_input_tokens` check
  requests against known models; dynamic discovery via `list_models()` (`GET /models`);
  `ModelRegistry` for managing your own catalogs.
- **Async runtime-agnostic** — built on `isahc` + `async-trait`; the library has no tokio
  dependency (tokio appears only in dev-dependencies for tests and examples).
- **Provider-private parameters** — `provider_options` passes vendor-specific JSON straight into
  the wire body (and can intentionally override standard fields).
- **Typed errors** — `ProviderError` distinguishes transport, I/O, auth, rate-limit, API, request
  validation, and wire encode/decode failures, with full `std::error::Error` support.

## Quick start

Add to `Cargo.toml`:

```toml
[dependencies]
oven-llm = "0.2"
```

A minimal non-streaming call:

```rust
use oven_llm::{CompletionsProvider, Message, Provider, Request, ThinkingMode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("DEEPSEEK_API_KEY")?;
    let provider = CompletionsProvider::deepseek(api_key);

    let request = Request::builder()
        .model("deepseek-v4-flash")
        .message(Message::user_text("Describe what Rustlang is"))
        .temperature(0.0)
        .thinking(ThinkingMode::Enabled)
        .build()?;

let response = provider.complete(&request).await?;
println!("{}", response.text());
Ok(())
}
```

### Unified provider creation (`ProviderBuilder`)

Instead of calling `CompletionsProvider::new` / `ResponsesProvider::new` separately, create any
provider through one entry point — `ProviderKind` + `ProviderName` + API key:

```rust
use oven_llm::{Provider, ProviderBuilder, ProviderKind, ProviderName};

// Option 1
let provider = ProviderBuilder::new(ProviderKind::Responses)          // or ProviderKind::Completions
    .provider_name(ProviderName::DeepSeek)
    .api_key(api_key)
    .build()?;                              // Box<dyn Provider>

// Option 2
let provider = ProviderBuilder::completions()          // or responses()
    .provider_name(ProviderName::DeepSeek)
    .api_key(api_key)
    .build()?;                              // Box<dyn Provider>

let response = provider.complete(&request).await?;
```

`build()` returns a `Result`, so unsupported kind/name combinations (e.g. Responses + Moonshot)
surface as a typed error instead of panicking. Known presets keep their base URL and static model
catalog, so `.known_models(...)` / `.add_model(...)` / `.extra_headers(...)` can augment them
without a `base_url`. Custom gateways work too: add `.base_url(...)` (optionally with
`.extra_headers(...)` / `.known_models(...)` / `.add_model(...)`), and any `ProviderName`,
including `Custom(...)`, is accepted.

## Supported protocols & providers

Each provider translates domain models to its own wire format via dedicated
`encoder` / `decoder` modules, so the protocol differences never leak into application code.

### OpenAI Chat Completions (compatible protocol)

`CompletionsProvider` implements the Chat Completions wire protocol, which a large ecosystem of
vendors speaks. Non-streaming responses decode `choices[0]`; streaming uses SSE terminated by a
`data: [DONE]` marker.

| Provider preset | Base URL | Static model catalog |
| --- | --- | --- |
| `CompletionsProvider::deepseek` | `https://api.deepseek.com` | `deepseek-v4-flash`, `deepseek-v4-pro` |
| `CompletionsProvider::moonshot` | `https://api.moonshot.cn/v1` | `kimi-k3`, `kimi-k2.7-code`, `kimi-k2.6` |
| `CompletionsProvider::zhipu` | `https://open.bigmodel.cn/api/paas/v4` | `glm-5.2`, `glm-5.1`, `glm-5`, `glm-4.7-flash` |
| `CompletionsProvider::openai` | `https://api.openai.com/v1` | *(empty — prefer `list_models()`)* |

### OpenAI Responses API

`ResponsesProvider` implements the newer Responses protocol. Streaming is SSE-based but has no
`[DONE]` marker: the stream terminates with `response.completed` / `response.incomplete` events,
and partial tool arguments arrive as `input_json` deltas.

| Provider preset | Base URL | Static model catalog |
| --- | --- | --- |
| `ResponsesProvider::deepseek` | `https://api.deepseek.com` | `deepseek-v4-flash` |
| `ResponsesProvider::grok` | `https://api.x.ai/v1` | `grok-build-0.1` |
| `ResponsesProvider::openai` | `https://api.openai.com/v1` | *(empty — prefer `list_models()`)* |

Anything not covered by a preset can be reached through `with_base_url(...)` /
`with_models(...)`, so any OpenAI-compatible gateway works.

## Examples

The repository ships three runnable examples. None of them requires a real API key — they fall
back to a placeholder key and print errors instead of panicking, so the full call flow can be
observed offline:

```sh
cargo run --example completions_usage
cargo run --example responses_usage
cargo run --example agent_loop -- "summarize this repository"
```

### Streaming with `StreamCollector`

```rust
use futures::StreamExt;
use oven_llm::{Delta, Provider, Request, StreamCollector, StreamEvent};

async fn stream_text(
    provider: &impl Provider,
    request: &Request,
) -> Result<oven_llm::Response, oven_llm::ProviderError> {
    let mut stream = provider.stream(request).await?;
    let mut collector = StreamCollector::new();

    while let Some(event) = stream.next().await {
        let event = event?;
        if let StreamEvent::ContentBlockDelta { delta, .. } = &event {
            match delta {
                Delta::ThinkingDelta { thinking } => print!("\x1b[90m{thinking}\x1b[0m"),
                Delta::TextDelta { text } => print!("{text}"),
                Delta::InputJsonDelta { .. } => {}
            }
        }
        collector.push(&event);
    }
    Ok(collector.finish()?)
}
```

### Tool-calling agent loop

```rust
use futures::StreamExt;
use oven_llm::{
    ContentBlock, Message, Provider, Request, ResponsesProvider, StopReason, StreamCollector,
};

async fn run_agent(
    provider: &ResponsesProvider,
    mut request: Request,
) -> Result<(), oven_llm::ProviderError> {
    loop {
        // 1. Stream the response and rebuild a complete `Response`.
        let mut stream = provider.stream(&request).await?;
        let mut collector = StreamCollector::new();
        while let Some(event) = stream.next().await {
            collector.push(&event?);
        }
        let response = collector.finish()?;

        // 2. Append the assistant message (with its tool_use blocks) first.
        request.message(Message::assistant(response.content.clone()));

        // 3. If the model didn't request a tool, we are done.
        if response.stop_reason != Some(StopReason::ToolUse) || !response.has_tool_use() {
            return Ok(());
        }

        // 4. Execute each tool and feed the results back.
        for block in response.tool_uses() {
            let ContentBlock::ToolUse { id, name, .. } = block else {
                continue;
            };
            println!("calling tool: {name}");
            request.message(Message::tool_result(id, "42", false));
        }
    }
}
```

### Custom gateway & provider options

```rust
use oven_llm::{CompletionsProvider, Message, Provider, ProviderName, Request};
use serde_json::json;

let provider = CompletionsProvider::with_base_url(
    "https://gateway.example.com/v1",
    ProviderName::Custom("my-gateway".into()),
    api_key,
);

let request = Request::builder()
    .model("my-model")
    .message(Message::user_text("hello"))
    .provider_option("extra_field", json!({ "enabled": true }))
    .build()?;
```

### Model registry & validation

```rust
use oven_llm::{ModelInfo, ModelRegistry, ProviderName};

let mut registry = ModelRegistry::new();
registry.register(ModelInfo::minimal("deepseek-chat", ProviderName::DeepSeek));

let by_provider = registry.list_by_provider(&ProviderName::DeepSeek);
let matches = registry.search("deepseek");
```

When a requested model ID hits a provider's static catalog, the request is validated against the
model's capabilities (context window, max output tokens, tool / vision / streaming support, …).
Unknown models pass through in a permissive mode and are left to the upstream service.

## Repository overview

| Path | Purpose |
| --- | --- |
| `src/domain/` | Provider-agnostic model: `Request` + builder, `Message` / `ContentBlock` / `Role`, `Response` / `StopReason` / `Usage`, `StreamEvent` / `Delta` / `StreamCollector`, `Tool` / `ToolChoice` |
| `src/provider/` | `Provider` trait, `ProviderError`, model metadata (`ModelInfo` / `ModelCapabilities` / `Pricing`), `ModelRegistry`, request validation (`validate_request` / `estimate_input_tokens`) |
| `src/provider/completions/` | Chat Completions wire types, encoder, decoder, static model catalogs, `CompletionsProvider` |
| `src/provider/responses/` | Responses API wire types, encoder, decoder, static model catalogs, `ResponsesProvider` |
| `src/provider/http.rs` | Shared `isahc` transport: headers, endpoint joining, status-code mapping, SSE bridge |
| `examples/` | Runnable examples: `completions_usage`, `responses_usage`, `agent_loop` |
| `scripts/` | Shell scripts + logs used while manually validating against real vendors |

## Contributing

Contributions are very welcome — bug reports, documentation, new provider presets, new wire
protocols, and model catalog updates all help the project grow.

Development workflow:

```sh
cargo fmt -- --check      # formatting must stay clean
cargo clippy --all-targets
cargo test                # CI also runs tests on ubuntu / macOS / windows
```

Guidelines:

- Keep the public API provider-agnostic: wire types live in provider `encoder` / `decoder`
  modules and must never leak through the `Provider` trait or domain types.
- Add tests with every behavior change — wire fixtures, stream state-machine cases, serde
  round-trips, and error mapping are all expected.
- Examples must stay runnable without a real API key.
- For behavior changes, open an issue or PR with a short description and the reasoning behind it.

Ideas for contributions:

- New provider presets (base URL + model metadata + pricing).
- Additional wire protocols, e.g. Anthropic Messages or Gemini.
- Vision / image input coverage for more providers.
- Retry & backoff policies built on `ProviderError::RateLimit`.
- Better token estimation in `estimate_input_tokens`.

## License

MIT — see [LICENSE](LICENSE).
