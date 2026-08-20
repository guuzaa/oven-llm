# oven-llm

[![crates.io](https://img.shields.io/crates/v/oven-llm.svg)](https://crates.io/crates/oven-llm)
[![docs.rs](https://docs.rs/oven-llm/badge.svg)](https://docs.rs/oven-llm)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust library for calling LLM providers through one unified, async runtime-agnostic API.

Build a vendor with [`ProviderBuilder`](#one-vendor-providerbuilder), register several of them on
a [`Router`](#several-vendors-router), and send `Request.model` as a `vendor/wire-id` slug.
Application code talks to the `Provider` trait and a provider-agnostic domain model
(`Request` / `Response` / `StreamEvent`). Wire formats stay inside the crate.

> [!WARNING]
> **Status: not production-ready.** This crate is under active development. The public API is
> still evolving and may change without notice, and there are no stability or correctness
> guarantees yet. Please evaluate it before using it in production workloads.

## Install

```sh
cargo add oven-llm
```

## One vendor (`ProviderBuilder`)

Do not pick a protocol. `ProviderBuilder::provider()` uses that vendor's default protocol
and returns `Box<dyn Provider>`. Use `completions()` / `responses()` to force one.

```rust
use oven_llm::{Provider, ProviderBuilder, ProviderName, Request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = ProviderBuilder::provider()
        .provider_name(ProviderName::DeepSeek)
        .api_key(std::env::var("DEEPSEEK_API_KEY")?)
        .build()?;

    let request = Request::builder()
        .model("deepseek/deepseek-v4-flash")
        .prompt("Describe what Rust is")
        .build()?;

    let response = provider.complete(&request).await?;
    println!("{}", response.text());
    Ok(())
}
```

`build()` returns a `Result`. An unsupported vendor (for example Anthropic, which has no
implementation yet) is a typed error, not a panic. Known presets keep their base URL and static
catalog; `.add_model(...)` / `.extra_headers(...)` can extend them. A custom gateway needs
`.base_url(...)` and any `ProviderName`, including `Custom(...)`.

Force a single wire protocol only when you need to:

```rust
let provider = ProviderBuilder::completions() // or ::responses()
    .provider_name(ProviderName::DeepSeek)
    .api_key(api_key)
    .build()?;
```

## Several vendors (`Router`)

Register each vendor once. `Request.model` decides where the call goes — mixing vendors is
configuration, not a `match` in application code.

```rust
use oven_llm::{ProviderBuilder, ProviderName, Request, Router};

let deepseek = ProviderBuilder::provider()
    .provider_name(ProviderName::DeepSeek)
    .api_key(deepseek_key)
    .build()?;
let zhipu = ProviderBuilder::provider()
    .provider_name(ProviderName::Zhipu)
    .api_key(zhipu_key)
    .build()?;

let mut router = Router::new();
router.register(deepseek).register(zhipu);

let request = Request::builder()
    .model("zhipu/glm-5.3")
    .prompt("hello")
    .build()?;

let response = router.complete(&request).await?;
let mut stream = router.stream(&request).await?;
```

Dispatch:

1. The vendor segment of the slug (`deepseek/...` → the DeepSeek registration).
2. Each provider's static catalog (first registration wins).
3. No match → `RouterError::UnknownModel`, never a silent fallback to the wrong vendor.

Prefer `vendor/wire-id` (`deepseek/deepseek-v4-flash`). A bare id is qualified when the router
only has one vendor. Protocol comes from the catalog, or from an optional `:responses` suffix —
callers do not pick `ProviderKind` per request.

`Router` itself implements `Provider`, so an agent can hold one object for both a single vendor
and a mix.

## Vendors

| `ProviderName` | slug | protocols | example model |
| --- | --- | --- | --- |
| `DeepSeek` | `deepseek` | Completions + Responses | `deepseek/deepseek-v4-flash` |
| `Moonshot` | `moonshot` | Completions | `moonshot/kimi-k3` |
| `Zhipu` | `zhipu` | Completions | `zhipu/glm-5.3` |
| `Grok` | `xai` | Responses | `xai/grok-4.6` |
| `OpenAI` | `openai` | Completions + Responses | *(empty catalog — use `list_models()`)* |
| `Custom(name)` | the name you pass | Completions (unless you set `kind`) | `my-proxy/local-llama` |

Anything not covered by a preset: `.base_url(...)` on `ProviderBuilder`.

```rust
let gateway = ProviderBuilder::provider()
    .provider_name(ProviderName::Custom("my-proxy".into()))
    .api_key(api_key)
    .base_url("https://gateway.example.com/v1")
    .build()?;
```

Streaming, tool-calling, thinking/reasoning, request validation, and vendor-specific
`provider_options` are all on the same `Provider` / `Request` types. See the examples rather
than another copy of the API here.

## Examples

None of these require a real API key — they fall back to a placeholder and print errors instead
of panicking:

```sh
cargo run --example router_usage
cargo run --example completions_usage
cargo run --example responses_usage
cargo run --example agent_loop -- "summarize this repository"
```

`router_usage` is the one that matches this README: `ProviderBuilder` + `Router` across DeepSeek,
Zhipu, and a custom gateway.

## Contributing

Bug reports, documentation, new provider presets, new wire protocols, and model catalog updates
are all welcome.

```sh
cargo fmt -- --check
cargo clippy --all-targets
cargo test
```

Keep the public API provider-agnostic: wire types stay in encoder/decoder modules. Add tests
with every behavior change. Examples must stay runnable without a real API key.

## License

MIT — see [LICENSE](LICENSE).
