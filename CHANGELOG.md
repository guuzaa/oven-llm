# Changelog

## [pending] - 2026-08-14
### Breaking
- Removed all provider-specific presets and endpoints: the static model
  catalogs (`completions::models` / `responses::models` and their
  `deepseek_models` / `moonshot_models` / `zhipu_models` / `grok_models`
  functions), the preset constructors (`CompletionsProvider::deepseek` /
  `moonshot` / `zhipu` / `openai`, `ResponsesProvider::deepseek` / `grok` /
  `openai`), the `new(ProviderName, ...)` dispatch constructors, the base URL
  constants, and `ProviderBuilder` preset dispatch. The crate ships no provider
  endpoints or model metadata.
- `base_url` is now required on every build path: `ProviderBuilder::build()`
  without it returns `InvalidProviderConfig`; `with_base_url` / `with_models`
  are the only provider constructors.

### Added
- `ModelRegistry::from_models`, `ModelRegistry::into_models`, and
  `ModelRegistry::unregister` — `ModelRegistry` is now the single entry point
  for users to maintain provider model lists.
- `ProviderBuilder::model_registry(...)` injects a maintained `ModelRegistry`
  into the built provider (same setting semantics as `known_models`, with
  `add_model` still appending).

### Changed
- Router's catalog-scan fallback and request validation now apply only to
  user-supplied model catalogs; README no longer lists provider presets or
  static models.

## [0.3.0] - 2026-08-14
### Added
- New `Router` routing layer: register multiple providers and dispatch
  `complete` / `stream` by `Request.model`, so callers maintain a single
  "model → provider" registration instead of switching providers manually.
  - Dispatch priority: exact `alias(...)` bindings, then the longest matching
    `route(...)` prefix (earliest rule wins ties), then each provider's static
    model catalog in registration order; no match returns
    `RouterError::UnknownModel` instead of silently routing to the wrong vendor.
  - `RouterError` with `NoProviderRegistered`, `UnknownModel`, and `Provider`
    variants; rules reference providers by `ProviderName` and resolve at
    dispatch time, so `route` / `alias` work before or after `register`.
- New `router_usage` example showing routing across DeepSeek / Zhipu providers.
- `Display` impls for `ProviderName`, `ReasoningEffort`, and `ThinkingMode`
  (wire-style lowercase strings for the latter two).
- `Sub` and `SubAssign` impls for `Usage`, with per-field saturating
  subtraction (complements the existing `Add` / `AddAssign`).

### Fixed
- `RequestBuilder::thinking(ThinkingMode::Enabled)` now defaults
  `reasoning_effort` to `Medium` when it hasn't been set explicitly, so
  enabling thinking no longer requires toggling `reasoning_effort`.
- Completions decoder falls back to `prompt_tokens_details.reasoning_tokens`
  when `completion_tokens_details.reasoning_tokens` is absent, since some
  providers report reasoning tokens under the prompt details.

### Changed
- Completions encoder builds `thinking` / `reasoning_effort` wire values via
  the new `Display` impls (same wire format, less duplicated matching).

## [0.2.2] - 2026-08-07
### Added
- `ProviderName` now uses a hand-written Serialize/Deserialize impl:
  - known providers serialize as lowercase strings ("openai", "deepseek", ...)
  - Custom serializes as "custom(<name>)" with the name normalized to lowercase
  - deserialization lowercases the whole input, so it is case-insensitive
  - unknown strings fall back to Custom for backward compatibility
- `ProviderKind` derives Serialize/Deserialize with lowercase variant names.

## [0.2.1] - 2026-08-03
### Added
- Unified provider creation: `ProviderBuilder` + `ProviderKind` dispatch to
  `CompletionsProvider` / `ResponsesProvider` from one entry point
  (`kind` + `provider_name` + `api_key`, optional `base_url` / `extra_headers` /
  `known_models`), returning `Result<Box<dyn Provider>>`.
- `ProviderError::UnsupportedProvider` and `ProviderError::InvalidProviderConfig`
  for builder failures (the unified path returns errors instead of panicking).
- `ProviderBuilder::add_model` to append a single `ModelInfo` to the builder's
  `known_models` list (complements the bulk `known_models(...)` setter).
- `ProviderBuilder` builds known presets from their preset base URL and static
  model catalog, so `known_models` / `add_model` / `extra_headers` can augment
  presets without requiring a `base_url`.

## [0.2.0] - 2026-08-02
### Added
- OpenAI Responses API support: `ResponsesProvider` with DeepSeek / Grok / OpenAI
  presets, wire types, encoder/decoder, SSE streaming, and a new
  `responses_usage` example.
- Re-export `secrecy::SecretString` as `oven_llm::SecretString` for callers that
  want to wrap API keys explicitly.

### Changed
- Rename `OpenAICompatProvider` to `CompletionsProvider` and the `openai_compat`
  module to `completions` (breaking).
- Remove the aggregate model-list APIs (`all_openai_compat_models`,
  `all_responses_models`) and their re-exports (breaking).
- Sort `ProviderError` variants into a stable, grouped order.
- Extract the shared HTTP transport into `provider/http.rs` (auth headers,
  endpoint joining, status-code mapping, SSE bridging, `list_models`) and use it
  from both providers.
- Provider constructors (`new`, `with_base_url`, `with_models`, and all vendor
  presets) now accept `impl Into<SecretString>`: callers can pass a plain
  `String` or `&str` without depending on `secrecy`; the key is still stored
  internally as `SecretString`.
- Rewrite the README with a feature overview, protocol descriptions, code
  examples, and a contribution guide; rename the `basic_usage` example to
  `completions_usage`.

### Fixed
- Non-streaming `complete` now decodes the first choice instead of rejecting
  responses with multiple `choices`, matching streaming behavior.
- Flaky Responses decoder tests on Windows: SSE test fixtures now handle CRLF
  line endings.

### Removed
- `CompletionsDecodeError::MultipleChoices` and its validation logic.
- Stale design-document references from module-level docs.

## [0.1.3] - 2026-07-30
### Fixed
- Polish Cargo.toml: cut unused files for package
- Replace reqwest with isahc, making this lib asynchronous runtime-agnostic

## [0.1.2] - 2026-07-28
### Added
- New APIs: 
    - OpenAICompatProvider: new, with_base_url
    - Message: system_prompt

### Fixed
- Polish tokio features: rt, macros, rt-multi-thread

## [0.1.1] - 2026-07-25

### Added
- New APIs: 
    - Response: thinking, text, tool_uses, has_tool_use
    - Message: system, user_text, assistant_text, assistant_text, tool_result

### Fixed
- Flaky tests in Provider

### Added

## [0.1.0] - 2026-07-23

### Added
- Initial Release
- Supports OpenAI compatible API
