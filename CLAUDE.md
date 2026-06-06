# GigaAM Desktop — Development Guidelines

## 1. Think Before Coding

Don't assume. Don't hide confusion. Surface tradeoffs.

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

Minimum code that solves the problem. Nothing speculative.

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

Touch only what you must. Clean up only your own mess.

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

Define success criteria. Loop until verified.

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

## Project-Specific Guidelines

### Architecture

- **Tauri 2.x** desktop app. Backend is Rust, frontend is Svelte 5 + TypeScript.
- Rust types use `snake_case`, serialized as `camelCase` via `serde(rename_all)`.
- Frontend types in `src/lib/types.ts` must mirror `src-tauri/src/types.ts` exactly.
- Config lives in `%APPDATA%/GigaAM/config.toml` (Windows) or `~/.config/GigaAM/` (Linux/macOS).

### Code Style

- **Rust**: standard rustfmt. Use `tracing` for logging, not `println!`.
- **TypeScript**: single quotes, semicolons, 2-space indent.
- **Svelte 5**: use runes (`$state`, `$derived`, `$effect`). No legacy `$:` syntax.
- Match the existing style in each file. When in doubt, look at neighboring code.

### Build & Test

- Frontend: `cd apps/desktop && npm run check` (type check).
- Backend: `cd apps/desktop/src-tauri && cargo clippy && cargo test`.
- Full Tauri build happens on GitHub Actions only (`release.yml`).
- Do not attempt to run `tauri build` locally.

### Pipeline

The main pipeline is: `URL → yt-dlp → ffmpeg → GigaAM ASR → txt/srt/json`

Each stage is in its own module (`ytdlp.rs`, `ffmpeg.rs`, `asr/`). The orchestrator is `pipeline.rs`.
