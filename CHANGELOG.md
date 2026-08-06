# Changelog

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
