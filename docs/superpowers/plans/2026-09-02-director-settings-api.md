# Director API settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A running buddy can be pointed at a different Completer from settings, and `cargo run` reuses the saved base URL, model, and API key without those env vars.

**Architecture:** Base URL and model persist on `Settings` in `settings.json`. The API key lives behind a `SecretStore` seam (`MemoryStore` in tests, `keyring` in the process). `model::resolve` / `config_from` / `endpoint_from` pick env-when-set over persisted values. Applying an endpoint patch writes the store, sends `SettingsOp::Retarget`, and the frame loop rebuilds every Instance's `ModelDirector` the way `switch_instance` already rebuilds on a Character change.

**Tech Stack:** Rust, Tauri 2, AppKit settings window, `keyring` for the OS secret store.

**Spec:** GitHub issue #214 (https://github.com/omesser/ai-buddy/issues/214). Domain language: `CONTEXT.md` (`Director`, Completer, Character Prompt). Settings user stories: `docs/SPEC.md` 68–70.

## TDD seams

Tests land only on these public boundaries:

1. `secrets::SecretStore` (`get` / `set` / `delete`) against `MemoryStore`
2. `Settings` save/load of `director_base_url` and `director_model`; serialized JSON never contains an API key
3. `SettingsView` carries `api_key_set` and `api_key_fingerprint`, never the raw key
4. `model::resolve` / `config_from` / `endpoint_from` (env vs persisted vs store)
5. `write_director_key` and `retarget_model` (store write, cancel in-flight, new Completer)

No tests against Keychain. No tests of AppKit widgets.

## Global Constraints

- Field names: `director_base_url`, `director_model` on `Settings`. Empty string means "not persisted" and falls through to env then defaults (`https://api.openai.com`, `gpt-4o-mini`).
- The API key is **not** a `Settings` field and **must not** appear in `settings.json`.
- Secret store service `ai-buddy`, account `director-api-key`.
- Env vars `AI_BUDDY_DIRECTOR_API_KEY`, `AI_BUDDY_DIRECTOR_BASE_URL`, `AI_BUDDY_DIRECTOR_MODEL`, when **set**, override that process and do not write through to the file or the secret store.
- Remote URL with no key → not configured (Static only). Local URL with no key → configured.
- Timeout, max tokens, `AI_BUDDY_DIRECTOR_WAKE_SECS` stay env-only.
- No settings webview. Linux/Windows settings UI is out of scope (#196, #197).
- Vocabulary: Director, Completer, Character Prompt — not "brain", "provider", "system prompt".
- Comments say why, not what (`docs/agents/comments.md`).
- Commit subjects: Conventional Commits, imperative, capitalized, no full stop. This branch squash-merges; keep subjects as the year-from-now line.
- **Key-field ruling:** the secure field is never filled from the store (avoids round-tripping the secret and accidental-delete-on-blur). A non-empty commit writes the store. A **Clear key** button sends `director_api_key: Some("")` to delete. An empty field on blur is `None` (leave stored key alone).
- Follow TDD: failing test first, watch it fail, then minimal code. Record RED/GREEN evidence in the task report.
- Work from this worktree only. Do not dispatch subagents.

---

### Task 1: Secret store seam

**Files:**
- Create: `src-tauri/src/secrets.rs`
- Modify: `src-tauri/src/main.rs` (add `mod secrets;`)
- Modify: `src-tauri/Cargo.toml` (add `keyring = "3"`)
- Test: `src-tauri/src/secrets.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: nothing from later tasks
- Produces:
  - `pub const DIRECTOR_API_KEY: &str = "director-api-key";`
  - `pub trait SecretStore: Send + Sync { fn get(&self, account: &str) -> Result<Option<String>, String>; fn set(&self, account: &str, value: &str) -> Result<(), String>; fn delete(&self, account: &str) -> Result<(), String>; }`
  - `pub struct MemoryStore` with `MemoryStore::new() -> Self` and `impl SecretStore`
  - `pub struct KeyringStore` with `KeyringStore::new() -> Self` (service `"ai-buddy"`) and `impl SecretStore`
  - `KeyringStore` is constructed only at process startup (Task 4). This task's tests use `MemoryStore` only.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_missing_item_is_none_not_an_error() {
    let store = MemoryStore::new();
    assert_eq!(store.get(DIRECTOR_API_KEY).unwrap(), None);
}

#[test]
fn a_set_item_is_what_get_returns() {
    let store = MemoryStore::new();
    store.set(DIRECTOR_API_KEY, "sk-test-key").unwrap();
    assert_eq!(
        store.get(DIRECTOR_API_KEY).unwrap().as_deref(),
        Some("sk-test-key")
    );
}

#[test]
fn deleting_an_item_leaves_none() {
    let store = MemoryStore::new();
    store.set(DIRECTOR_API_KEY, "sk-test-key").unwrap();
    store.delete(DIRECTOR_API_KEY).unwrap();
    assert_eq!(store.get(DIRECTOR_API_KEY).unwrap(), None);
}

#[test]
fn deleting_a_missing_item_is_ok() {
    let store = MemoryStore::new();
    store.delete(DIRECTOR_API_KEY).unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ai-buddy -- secrets`

Expected: FAIL compiling (`secrets` module / `MemoryStore` not found).

- [ ] **Step 3: Write minimal implementation**

`MemoryStore` is a `Mutex<HashMap<String, String>>`. `get` clones the value. `delete` of a missing key succeeds.

`KeyringStore`:

```rust
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self { service: "ai-buddy".to_string() }
    }
}
```

`get` maps `keyring::Error::NoEntry` to `Ok(None)`. `set` / `delete` use `Entry::new(&self.service, account)`. Do not call Keychain from tests.

Add `keyring = "3"` to `src-tauri/Cargo.toml` with a why-comment: the OS secret store is how the Director key survives `cargo run` without landing in `settings.json`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ai-buddy -- secrets`

Expected: PASS, four tests, output pristine.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/secrets.rs src-tauri/src/main.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
feat(shell): Store the Director API key behind a SecretStore seam

EOF
)"
```

---

### Task 2: Persist base URL and model, expose them on the view

**Files:**
- Modify: `src-tauri/src/settings.rs`
- Test: `src-tauri/src/settings.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: Task 1's `SecretStore` is **not** used yet. This task is the JSON document and the snapshot the window will read.
- Produces:
  - `Settings.director_base_url: String` (default `""`)
  - `Settings.director_model: String` (default `""`)
  - `SettingsPatch.director_base_url: Option<String>`
  - `SettingsPatch.director_model: Option<String>`
  - `SettingsPatch.director_api_key: Option<String>` — present on the patch so later tasks can write the store; `Settings::apply` **ignores** it (the key is not a file field)
  - `SettingsView.director_base_url: String`
  - `SettingsView.director_model: String`
  - `SettingsView.api_key_set: bool`
  - `SettingsView.api_key_fingerprint: String`
  - `SettingsView::from_parts` grows two extra args: `api_key_set: bool`, `api_key_fingerprint: String`
  - `pub fn fingerprint(key: &str) -> String` — same format as today's `Endpoint::key_fingerprint` (`len={n} last={last}`). Put it in `model.rs` as `pub fn key_fingerprint(key: &str) -> String` and have `Endpoint::key_fingerprint` call it. If moving it in this task would fight Task 3, duplicate the one-liner in settings tests by passing the fingerprint string in; Task 3 owns the shared function.

- [ ] **Step 1: Write the failing tests**

Extend `a_saved_document_round_trips` with `director_base_url: "https://api.x.ai".into()`, `director_model: "grok-4.6".into()`.

Add:

```rust
#[test]
fn a_partial_document_fills_missing_director_endpoint_from_defaults() {
    let path = temp_path();
    fs::write(&path, r#"{"director_enabled":false}"#).expect("write");
    let settings = Settings::load(&path);
    assert!(settings.director_base_url.is_empty());
    assert!(settings.director_model.is_empty());
    let _ = fs::remove_file(&path);
}

#[test]
fn the_saved_document_does_not_carry_an_api_key() {
    let path = temp_path();
    let settings = Settings {
        director_base_url: "https://api.x.ai".to_string(),
        director_model: "grok-4.6".to_string(),
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    let text = fs::read_to_string(&path).expect("read");
    assert!(!text.contains("api_key"), "{text}");
    assert!(!text.contains("sk-"), "{text}");
    let _ = fs::remove_file(&path);
}

#[test]
fn the_settings_view_never_holds_the_raw_key() {
    let settings = Settings {
        director_base_url: "https://api.x.ai".to_string(),
        director_model: "grok-4.6".to_string(),
        ..Settings::default()
    };
    let view = SettingsView::from_parts(
        &settings,
        Path::new("/tmp/ai-buddy/memory.md"),
        Some("You are Nim.".to_string()),
        vec!["nim".to_string()],
        Vec::new(),
        true,
        "len=12 last=key1".to_string(),
    );
    assert_eq!(view.director_base_url, "https://api.x.ai");
    assert_eq!(view.director_model, "grok-4.6");
    assert!(view.api_key_set);
    assert_eq!(view.api_key_fingerprint, "len=12 last=key1");
    let dump = format!("{view:?}");
    assert!(!dump.contains("sk-"), "{dump}");
}

#[test]
fn apply_ignores_the_api_key_patch_on_the_file() {
    let mut settings = Settings::default();
    settings.apply(SettingsPatch {
        director_base_url: Some("https://api.x.ai".to_string()),
        director_model: Some("grok-4.6".to_string()),
        director_api_key: Some("sk-should-not-land".to_string()),
        ..SettingsPatch::default()
    });
    assert_eq!(settings.director_base_url, "https://api.x.ai");
    assert_eq!(settings.director_model, "grok-4.6");
    let path = temp_path();
    settings.save(&path).expect("save");
    let text = fs::read_to_string(&path).expect("read");
    assert!(!text.contains("sk-should-not-land"), "{text}");
    let _ = fs::remove_file(&path);
}
```

Update every existing `Settings { ... }` literal and `SettingsView::from_parts` call in this file so they compile once the new fields exist — do that in Step 3, not by weakening the new tests.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ai-buddy -- settings`

Expected: FAIL (unknown fields / `from_parts` arity).

- [ ] **Step 3: Write minimal implementation**

Add the fields. `#[serde(default)]` already on `Settings` fills missing keys. Do **not** add an `api_key` field to `Settings`.

Update `the_settings_view_is_what_the_window_shows` to pass `false` and `""` for the new `from_parts` args, and assert `!view.api_key_set`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ai-buddy -- settings`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs
git commit -m "$(cat <<'EOF'
feat(shell): Persist Director base URL and model on Settings

EOF
)"
```

---

### Task 3: Resolve Completer config from env, settings, and the store

**Files:**
- Modify: `src-tauri/src/model.rs`
- Test: `src-tauri/src/model.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Settings.director_base_url` / `director_model` (strings). Stored key as `Option<&str>` — this task does not depend on `SecretStore` at the `resolve` signature.
- Produces:
  - `pub struct DirectorSources { pub base_url: String, pub model: String, pub api_key: String, pub key_invalid: bool }`
    - `api_key` empty means unset/invalid. `key_invalid` true means the winning source was set but unusable (today's `KeyRead::Invalid`).
  - `pub fn resolve(persisted_base: &str, persisted_model: &str, stored_key: Option<&str>) -> DirectorSources`
  - `pub fn config_from(sources: &DirectorSources) -> DirectorConfig`
  - `pub fn endpoint_from(sources: &DirectorSources) -> Option<Endpoint>`
  - `pub fn key_fingerprint(key: &str) -> String`
  - Keep `config()` and `endpoint()` as wrappers: `config_from(&resolve("", "", None))` and `endpoint_from(&resolve("", "", None))` so existing env-only tests and `spawn_preflight` keep working until Task 4 rewires them.
  - `is_local`, timeout, max tokens stay as they are, keyed off the **resolved** base URL.

Resolution:

- Base URL: env `AI_BUDDY_DIRECTOR_BASE_URL` if set (including empty → treat as unset and fall through); else persisted if non-empty; else `https://api.openai.com`.
- Model: same for `AI_BUDDY_DIRECTOR_MODEL` / persisted / `gpt-4o-mini`.
- Key: `key_from_env()` if not `Unset`; else `key_from_raw(stored_key)`. Env Invalid still wins over a stored key (the process asked to override).
- `configured` = present key OR `is_local(&base_url)`.
- `enabled` = `configured && !off()` (`AI_BUDDY_DIRECTOR` still env-only, already on `Settings.director_enabled` at the shell).

- [ ] **Step 1: Write the failing tests**

These tests must not leak env into each other. Save and restore the three vars around each test, or set them explicitly and `remove_var` the ones that should be unset.

```rust
fn with_env(
    key: Option<&str>,
    base: Option<&str>,
    model: Option<&str>,
    body: impl FnOnce(),
) {
    // save, set or remove, run, restore
}

#[test]
fn env_beats_persisted_base_and_model() {
    with_env(
        None,
        Some("https://api.x.ai"),
        Some("grok-4.6"),
        || {
            let sources = resolve(
                "https://api.openai.com",
                "gpt-4o-mini",
                Some("sk-stored"),
            );
            assert_eq!(sources.base_url, "https://api.x.ai");
            assert_eq!(sources.model, "grok-4.6");
        },
    );
}

#[test]
fn persisted_is_used_when_env_is_unset() {
    with_env(None, None, None, || {
        let sources = resolve("https://api.x.ai", "grok-4.6", Some("sk-stored-key"));
        assert_eq!(sources.base_url, "https://api.x.ai");
        assert_eq!(sources.model, "grok-4.6");
        assert_eq!(sources.api_key, "sk-stored-key");
        assert!(!sources.key_invalid);
    });
}

#[test]
fn env_key_beats_the_stored_key() {
    with_env(Some("sk-env-key"), None, None, || {
        let sources = resolve("", "", Some("sk-stored-key"));
        assert_eq!(sources.api_key, "sk-env-key");
    });
}

#[test]
fn a_remote_url_without_a_key_is_not_configured() {
    with_env(None, None, None, || {
        let sources = resolve("https://api.openai.com", "gpt-4o-mini", None);
        let config = config_from(&sources);
        assert!(!config.configured);
        assert!(endpoint_from(&sources).is_none());
    });
}

#[test]
fn a_local_url_without_a_key_is_configured() {
    with_env(None, None, None, || {
        let sources = resolve("http://localhost:11434", "gemma4", None);
        let config = config_from(&sources);
        assert!(config.configured);
        let endpoint = endpoint_from(&sources).expect("local needs no key");
        assert!(endpoint.url().contains("11434"));
        assert_eq!(endpoint.model(), "gemma4");
    });
}

#[test]
fn resolve_does_not_write_env() {
    with_env(None, None, None, || {
        let _ = resolve("https://api.x.ai", "grok-4.6", Some("sk-stored"));
        assert!(std::env::var("AI_BUDDY_DIRECTOR_API_KEY").is_err());
        assert!(std::env::var("AI_BUDDY_DIRECTOR_BASE_URL").is_err());
    });
}
```

`with_env` must restore even on panic (`defer` via a drop guard).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ai-buddy -- model::tests::env_beats cargo test -p ai-buddy -- model::tests::persisted_is_used cargo test -p ai-buddy -- model::tests::env_key_beats cargo test -p ai-buddy -- model::tests::a_remote_url_without cargo test -p ai-buddy -- model::tests::a_local_url_without cargo test -p ai-buddy -- model::tests::resolve_does_not`

Or one filter: `cargo test -p ai-buddy -- model::tests::`

Expected: FAIL (`resolve` / `config_from` / `endpoint_from` not found). Existing model tests must still compile — add the new APIs without breaking `config()` / `endpoint()`.

- [ ] **Step 3: Write minimal implementation**

Replace the env-only `base_url()` / `secret_key()` uses inside `config` / `endpoint` by routing through `resolve` / `config_from` / `endpoint_from`. `Endpoint::key_fingerprint` calls `key_fingerprint(&self.api_key)`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ai-buddy -- model`

Expected: PASS, including the new tests and the existing ones.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/model.rs
git commit -m "$(cat <<'EOF'
feat(shell): Resolve the Director Completer from env, settings, and the store

EOF
)"
```

---

### Task 4: Write the key on apply, retarget running Instances

**Files:**
- Modify: `src-tauri/src/settings.rs` (`SettingsSession`, `SettingsOp`, `write_director_key`, `view`)
- Modify: `src-tauri/src/main.rs` (`mod` already added; `SettingsSession` construction; `FrameExtras`; `SettingsOp::Retarget` in the frame loop; `switch_instance` / `spawn_live` / `spawn_instances` use `endpoint_from`)
- Test: `src-tauri/src/settings.rs` for `write_director_key`; `src-tauri/src/model.rs` or `src-tauri/src/main.rs` for `retarget_model`

**Interfaces:**
- Consumes: `SecretStore`, `DIRECTOR_API_KEY`, `resolve`, `config_from`, `endpoint_from`, `key_fingerprint`
- Produces:
  - `pub fn write_director_key(store: &dyn SecretStore, patch: &SettingsPatch) -> Result<(), String>`
    - `None` → no-op
    - `Some("")` → `store.delete(DIRECTOR_API_KEY)`
    - `Some(value)` → `store.set(DIRECTOR_API_KEY, value)` after the same trim `model::` already uses on env keys; empty after trim is delete
  - `SettingsOp::Retarget`
  - `SettingsSession.secrets: Arc<dyn SecretStore>`
  - `SettingsSession::view` reads the store: `api_key_set` / fingerprint from `store.get(DIRECTOR_API_KEY)` (fingerprint via `model::key_fingerprint`). Never puts the raw key on the view.
  - `SettingsSession::apply`: after `settings.apply(patch)`, call `write_director_key`; if `patch.director_base_url`, `director_model`, or `director_api_key` is `Some`, send `SettingsOp::Retarget`.
  - `pub fn retarget_model(pending: &mut InFlight, in_flight: &mut Option<Context>, model: &mut Option<Arc<ModelDirector<Endpoint>>>, behaviors: impl IntoIterator<Item = impl Into<String>>, sources: &DirectorSources, enabled: bool)`
    - `pending.cancel()`
    - `*in_flight = None`
    - `*model = enabled.then(|| Arc::new(ModelDirector::new(endpoint_from(sources).expect("enabled means configured"), behaviors)))`
  - Frame loop on `Retarget`: recompute `config` from current settings + store (`config.enabled = settings.director_enabled && config.configured`), then `retarget_model` on every live Instance with that Instance's behavior keys. Same cancel comment as `switch_instance`.
  - Startup: `let secrets: Arc<dyn SecretStore> = Arc::new(KeyringStore::new());` load stored key, `resolve(&settings.director_base_url, &settings.director_model, stored.as_deref())`, `config_from`, `endpoint_from`. Replace `model::config()` / `model::endpoint()` at `show_settings`, `switch_instance`, `spawn_live`, `spawn_instances`, `spawn_preflight`.
  - `FrameExtras` (or `SettingsState`) holds `Arc<dyn SecretStore>` so the loop and `show_settings` share it.
  - `switch_instance` / `spawn_live` need the current sources, not a fresh env-only `endpoint()`. Pass `DirectorSources` (or rebuild from settings+store inside the loop, which already has both).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn write_director_key_sets_the_store_and_not_the_file() {
    let store = MemoryStore::new();
    let patch = SettingsPatch {
        director_api_key: Some("sk-from-settings".to_string()),
        ..SettingsPatch::default()
    };
    write_director_key(&store, &patch).unwrap();
    assert_eq!(
        store.get(DIRECTOR_API_KEY).unwrap().as_deref(),
        Some("sk-from-settings")
    );
}

#[test]
fn write_director_key_clears_on_empty() {
    let store = MemoryStore::new();
    store.set(DIRECTOR_API_KEY, "sk-from-settings").unwrap();
    let patch = SettingsPatch {
        director_api_key: Some(String::new()),
        ..SettingsPatch::default()
    };
    write_director_key(&store, &patch).unwrap();
    assert_eq!(store.get(DIRECTOR_API_KEY).unwrap(), None);
}

#[test]
fn write_director_key_none_leaves_the_store() {
    let store = MemoryStore::new();
    store.set(DIRECTOR_API_KEY, "sk-from-settings").unwrap();
    write_director_key(&store, &SettingsPatch::default()).unwrap();
    assert_eq!(
        store.get(DIRECTOR_API_KEY).unwrap().as_deref(),
        Some("sk-from-settings")
    );
}
```

In `model.rs` (keeps `main.rs` tests from constructing a Character):

```rust
#[test]
fn retarget_drops_an_in_flight_wake_and_installs_the_new_completer() {
    with_env(None, None, None, || {
        let sources = resolve("http://localhost:11434", "gemma4", None);
        let mut config = config_from(&sources);
        config.enabled = true;
        let mut pending = InFlight::new();
        let mut in_flight = Some(Context { /* use a cheap Context from existing helpers if any; else a Default if it exists */ });
        let mut model = None;
        retarget_model(
            &mut pending,
            &mut in_flight,
            &mut model,
            ["stroll"],
            &sources,
            config.enabled,
        );
        assert!(pending.ready(), "cancel replaced the channel");
        assert!(in_flight.is_none());
        assert!(model.is_some());
    });
}

#[test]
fn retarget_to_a_remote_without_a_key_leaves_static() {
    with_env(None, None, None, || {
        let sources = resolve("https://api.openai.com", "gpt-4o-mini", None);
        let config = config_from(&sources);
        let mut pending = InFlight::new();
        let mut in_flight = None;
        let mut model = None;
        retarget_model(
            &mut pending,
            &mut in_flight,
            &mut model,
            ["stroll"],
            &sources,
            config.enabled && config.configured,
        );
        assert!(model.is_none());
    });
}
```

If `Context` has no easy constructor, put `retarget_model` in `model.rs` taking `in_flight: &mut Option<()>` **no** — keep `Option<Context>`. Read `crates/core/src/director.rs` for `Context` fields and fill the same way existing director tests do (`fn context(...)`).

If `retarget_model` in `model.rs` would require `use` of `Context` already there: `model.rs` already imports `Context`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ai-buddy -- write_director_key` and `cargo test -p ai-buddy -- retarget`

Expected: FAIL (functions not found).

- [ ] **Step 3: Write minimal implementation**

Wire `SettingsSession` / frame loop / startup as specified. `spawn_preflight` currently reads `endpoint()`; pass the resolved `Endpoint` or call `endpoint_from(&sources)`.

`switch_instance` currently `model::endpoint().expect(...)`. After this, it uses the same `sources` the loop just computed for Retarget.

Do not log the raw key. Startup lines already fingerprint.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ai-buddy -- settings` and `cargo test -p ai-buddy -- model` and `cargo test -p ai-buddy -- --test-threads=1` if env tests need isolation.

Also run: `cargo test -p ai-buddy`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs src-tauri/src/main.rs src-tauri/src/model.rs
git commit -m "$(cat <<'EOF'
feat(shell): Retarget a running Director when settings change the Completer

EOF
)"
```

---

### Task 5: Settings window rows and README

**Files:**
- Modify: `src-tauri/src/platform/macos/settings_window.rs`
- Modify: `src-tauri/Cargo.toml` (`objc2-app-kit` features: add `"NSSecureTextField"` if `NSSecureTextField` needs it)
- Modify: `README.md` (the env table / `cargo run` examples)
- Test: no new AppKit tests. Existing `settings` view tests already cover what the window reads. If `from_parts` arity changed in Task 2, this task must not break them.

**Interfaces:**
- Consumes: `SettingsView.director_base_url`, `director_model`, `api_key_set`, `api_key_fingerprint`; `SettingsPatch` fields from Task 2; `SettingsSession::apply`
- Produces: three rows under the existing **Director** heading, before "Last user turn":
  - Base URL: `NSTextField::textFieldWithString`, placeholder `https://api.openai.com`
  - Model: same, placeholder `gpt-4o-mini`
  - API key: `NSSecureTextField` (or `NSTextField` configured as secure), never filled from the view. Placeholder when `api_key_set`: `Set — {fingerprint}`. Placeholder when not set: `Not set`. **Clear key** `NSButton` next to it, action sends `director_api_key: Some("".into())`.
  - Commit URL and model on end-editing (`sel!(commitEndpoint:)`): patch `director_base_url` / `director_model` from the field strings (trimmed). If the secure field is **non-empty**, also set `director_api_key: Some(trimmed)`. If it is empty, leave `director_api_key: None`.
  - Grow `DOC_HEIGHT` by ~140 so the new rows fit; keep `WINDOW_HEIGHT` unless the scroll is cramped.
  - `refresh` writes URL and model field strings from the view. Does **not** write into the secure field (leave empty). Update its placeholder from `api_key_set` / fingerprint.
  - Inspect panel unchanged: last user turn, never the key.

README:

- After the env examples, add that Settings → Director persists base URL and model, and stores the API key in the OS secret store (Keychain on macOS). `cargo run` with those env vars unset uses the saved Completer.
- Env vars remain a one-process override.
- Do not paste example keys.

- [ ] **Step 1: Write the failing test**

No AppKit test. Add one README-adjacent assertion only if you can do it from `SettingsView` — already done in Task 2.

If `commitEndpoint` needs a compile check: `cargo test -p ai-buddy --lib` does not exist (bin). Compile with `cargo test -p ai-buddy -- settings` which builds the bin.

- [ ] **Step 2: Implement the window and README**

Match existing helpers: `cursor.heading` / `hint` / `place`. New ivars: `base_url`, `model`, `api_key` (`RefCell<Option<Retained<NSTextField>>>`), `clear_key` button.

`textDidEndEditing` currently only commits excluded applications. Do not route endpoint fields through that unless you can tell the notification's object apart. Prefer target/action on the three fields (`setTarget` / `setAction` / `sel!(commitEndpoint:)`).

- [ ] **Step 3: Run tests**

Run: `cargo test -p ai-buddy`

Expected: PASS. The crate must compile on macOS (NSSecureTextField).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/platform/macos/settings_window.rs src-tauri/Cargo.toml src-tauri/Cargo.lock README.md
git commit -m "$(cat <<'EOF'
feat(shell): Configure the Director Completer from the settings window

EOF
)"
```

---

## Spec coverage

| #214 requirement | Task |
|---|---|
| Settings set base URL, model, API key | 2, 4, 5 |
| Running Instance next wake uses the new Completer | 4 |
| `cargo run` with env unset uses saved API | 3, 4 (KeyringStore at startup) |
| `settings.json` never contains the key | 2, 4 |
| Clearing the key removes the store item; remote → Static, local still works | 4 (`write_director_key` empty), 3 (configured rules) |
| Env override does not write through | 3, 4 |
| Inspect panel is last user turn, never the key | 5 (no change to payload), 2 (view) |
| Tests listed in the issue | 1–4 |

## Placeholder scan

No TBD/TODO. `with_env` and `Context` construction are specified as "match existing model/director tests".
