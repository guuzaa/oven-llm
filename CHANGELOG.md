# Changelog

## [0.4.1] - 2026-08-20
### Added
- `Router::upsert`: replace an existing registration with the same vendor slug,
  or append. Same-vendor multi-protocol still uses `register` (append).

### Changed
- `ProviderBuilder::provider()`: omit `kind` and the builder uses that vendor's
  default protocol. `completions()` / `responses()` still force a single protocol.

## [0.4.0] - 2026-08-19
### Added
- `ModelId` slug parsing: `vendor()` / `wire_id()` / `variant()` / `qualify()`,
  plus `canonical_vendor` (`kimi`→`moonshot`, `glm`→`zhipu`, `grok`→`xai`).
  `Request.model` is now `vendor/wire-id[:variant]` or a bare wire id.
- `Router::qualify` completes a bare id when only one vendor is registered, and
  rewrites vendor aliases (`grok/...` → `xai/...`).
- `Router` implements `Provider`, so an agent can hold one object for a single
  vendor or a mix. `known_models` / `list_models` return slugs and de-duplicate
  across registrations.
- `ProviderBuilder::provider()`: omit `kind` and the builder registers every
  protocol that vendor speaks (DeepSeek → Completions + Responses) as a
  `Router`. `completions()` / `responses()` still force a single protocol.
- `Provider::protocol`, `ProviderName::slug` / `matches_vendor`, and
  `ModelInfo::{protocols, default_protocol, supports_protocol, slug}` so
  protocol selection lives on the catalog instead of the request.
- `ProviderError::NoProviderRegistered` and `ProviderError::UnknownModel`,
  with `From<RouterError>` so `Router` can implement `Provider`.

### Changed
- Router dispatch is slug vendor first (`deepseek/...` → the DeepSeek
  registration), then each provider's static catalog (first registration
  wins). A `:responses` / `:messages` suffix, or the catalog default, picks
  the protocol. No match → `RouterError::UnknownModel`; an explicit vendor
  that is registered is forwarded even if the wire id is not in the catalog.
- Completions and Responses encoders send `ModelId::wire_id()` on the wire,
  not the full slug.
- `ProviderName::Grok` serializes as `"xai"`; `"xai"` and `"grok"` both
  deserialize back to `Grok`.
- Setting `base_url` on `ProviderBuilder` no longer drops the preset catalog;
  extra models append.
- README rewritten around `ProviderBuilder` + `Router` + `vendor/wire-id`
  slugs. `router_usage` now also registers a custom `my-proxy` gateway.

### Removed
- `Router::alias` and `Router::route`, along with the `aliases` / `prefixes`
  fields. Dispatch is slug vendor + static catalog only.
- `ModelRegistry`. Model lookup lives on each provider's static catalog
  (`known_models` / `resolve_model`) and `Router`; callers that need a custom
  list should pass `ModelInfo` through `ProviderBuilder`.

## [0.3.1] - 2026-08-16
### Added
- `RequestBuilder::prompt(...)` appends a user text message, so simple
  single-turn requests no longer need `Message::user_text` at the call site.
- `From<&str>` for `ProviderName`: case-insensitive parsing with aliases
  (`kimi` → Moonshot, `glm` → Zhipu); unknown strings become `Custom`.
- `Display` for `ProviderKind` (lowercase slugs: `completions` / `responses` /
  `messages`).

### Fixed
- Completions encoder now forwards assistant `Thinking` blocks as
  `reasoning_content` on the wire (concatenating interleaved thinking chunks),
  so multi-turn conversations keep the thinking prefix consistent with the
  previous turn instead of dropping it.

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
