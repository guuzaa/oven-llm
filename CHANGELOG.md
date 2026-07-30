# Changelog

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
