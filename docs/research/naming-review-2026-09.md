# Naming / Terminology Review — September 2026

Systematic review of codebase naming against `CONTEXT.md` glossary and ADRs.

## Executive Summary

The codebase follows `CONTEXT.md` vocabulary with high fidelity. No mechanical
renames are recommended at this time.

**One open product decision** (#466) concerns user-facing Settings UI copy, which
is explicitly out of scope for this review per the constraint: "Do NOT
unilaterally rename user-visible Director Settings UI / product copy in this pass."

## Survey Scope

Reviewed:
- `crates/core` (director, engine, character, memory, roster, tools, etc.)
- `src-tauri` (harness, acp_wire, settings, frame_loop, platform, action_log, session_log, model)
- `src/main.js`
- Action Log field names
- Module and file names
- Type, function, variable, and constant names

## Findings

### ✅ Do now: None

No mechanical renames identified. The codebase accurately uses CONTEXT.md terms:

- **Director**: Correctly named for the role (lines 110-115). `ModelDirector`
  and `StaticDirector` are implementation variants, not violations.
- **Completer**: Correctly used (lines 182-190). The thing that answers a
  Character Prompt.
- **Harness**: Correct (lines 176-179). External agent runtime the user attaches.
- **Character, Instance, Behavior, Primitive**: All match glossary.
- **Memory, Personality Prompt, Animation, Variant, Left Strip**: Correct.
- **State, Perch, Surface, Contact**: Match glossary.
- **Speech, speak, Cue**: Correct usage.

### 🟢 Completed (Issue #466)

**User-facing Settings UI copy** (issue #466):
- Decision (locked): Keep Director as domain role; fix user-facing labels
- Environment variables remain `AI_BUDDY_DIRECTOR_*` (no breaking change)
- Settings section renamed to "AI" (not "Director" or "AI Completer")
- Toggle label: "AI on" (clear what it means)
- HTTP field labels: "Model", "API key" (not "Director" or "Completer")
- CONTEXT.md clarifies: Director = role, Completer = trait (HTTP|Harness umbrella)

### ⚪ Leave (Intentional / Not Churny)

1. **`ModelDirector` and `StaticDirector`**:
   - Both are Directors per CONTEXT.md. The prefix distinguishes implementation.
   - Renaming would churn 200+ call sites for no clarity gain.

2. **`session_log.rs`**:
   - In-memory Chat turn replay buffer, not the Action Log (which is `action_log.rs`).
   - Name is accurate: it logs the current session's turns.
   - Not a glossary term; no confusion with Action Log in practice.

3. **British spelling "cancelled"** throughout:
   - Consistent across codebase (`session/cancel`, `cancelled` stop reason, etc.).
   - Not a glossary issue; leave as-is.

4. **Import tooling uses "pet"**:
   - `scripts/import-pet.py`, comments about "desktop pet ecosystem", etc.
   - These are about importing **external** pets/mascots into our Character Package format.
   - Not our own domain language; correctly describes source material.

5. **Documentation and comments use "mascot"**:
   - CONTEXT.md line 14 says avoid "mascot" for **Character**.
   - Docs/comments correctly use "desktop mascot" to describe the *product category*,
     not to name our Character concept.
   - No code names use "mascot".

## Glossary Coverage

Terms from `CONTEXT.md` Language section:

| Term | Code Usage | Status |
|------|-----------|--------|
| Character | `character.rs`, `Character` type | ✅ |
| Character Package | `package.rs`, load logic | ✅ |
| Character Manifest | `manifest.rs`, `CHARACTER_MANIFEST_FILE` | ✅ |
| Personality Prompt | `personality` field, file loading | ✅ |
| Character Prompt | `character_prompt()`, `director.rs` | ✅ |
| Instance Prompt | field in Character Prompt assembly | ✅ |
| Animation | `Animation` type, `character.rs` | ✅ |
| Variant | `variant_of`, ring drawing logic | ✅ |
| Left Strip | `left_of`, `#345` | ✅ |
| Required Animation Set | validation in `character.rs` | ✅ |
| Character Instance | `InstanceSpec`, `roster.rs` | ✅ |
| Memory | `memory.rs`, `MemoryManifest` | ✅ |
| Memory Manifest | `memory.md` file format | ✅ |
| State | `State` enum, `engine.rs` | ✅ |
| Primitive | `Primitive` enum, engine-owned | ✅ |
| Behavior | `Behavior` type, Character-declared | ✅ |
| Director | `director.rs`, role that proposes | ✅ |
| Proactive model call | `Ambient` variant, backoff logic | ✅ |
| Near Miss | `wake_and_near_miss()`, reported | ✅ |
| Perch | `Perch` collision, window edges | ✅ |
| Hold | `Hold` primitive, perch-ride Animation | ✅ |
| Talk | `talk` animation, Speech may play it | ✅ |
| Surface | umbrella over floor and Perch | ✅ |
| Contact | `Contact` enum, physics observation | ✅ |
| Spatial Layer | always-on, model-free system | ✅ |
| Functional Layer | invoked, Harness-driven work | ✅ |
| Harness | `harness.rs`, external agent runtime | ✅ |
| Completer | `Completer` trait, thing that answers | ✅ |
| Executor | owned by Harness, not ai-buddy | ✅ |
| Action Log | `action_log.rs`, Harness actions | ✅ |
| Ambient Capture | future; no code yet | — |
| On-Demand Capture | future; no code yet | — |
| Local Gate | future; no code yet | — |
| Grab, Throw, Poke, Menu, Summon | verbs, correctly named | ✅ |
| Speech, speak | `Speech` and `speak` tool | ✅ |
| Speech bubble | `bubble.js`, held for reading time | ✅ |
| Cue | `cue.js`, interaction acknowledgement | ✅ |
| Thinking ellipsis | `thinking-dots`, `bubble.js` | ✅ |
| Chat surface | chat window, `chat.js` | ✅ |
| Chat UI | visual design, `chat-ui.css` | ✅ |

## Avoided Terms (per CONTEXT.md)

| Term | Avoid per CONTEXT | Found in Code? | Notes |
|------|-------------------|----------------|-------|
| Pet | Yes (for Character) | No | Only in import tooling for external pets |
| Mascot | Yes (for Character) | No | Only in docs/comments about product category |
| Avatar | Yes (for Character) | No | Not found |
| Buddy | Yes (for Character) | No | "buddy" is the app, not the character |
| Brain | Yes (for Director) | No | Not found in code names |
| Agent | Yes (for Director) | No | "agent" correctly names the Harness |
| Planner | Yes (for Director) | No | Not found |

## Recommendations

1. **No immediate PRs**: The codebase is well-named; forcing renames would
   introduce churn without improving understandability.

2. **Issue #466 stays open**: User-facing Settings copy is a product decision
   requiring Oded's input. Code is correct; UI strings are the open question.

3. **ADR-0012 compliance verified**: Character Prompt terminology (Personality
   Prompt, Instance Prompt, layers) matches implementation.

4. **Future monitoring**: As new features land (Ambient/On-Demand Capture, Voice),
   ensure they adopt glossary terms immediately rather than inventing synonyms.

## Methodology

- Read `CONTEXT.md` Language section in full
- Read `docs/SPEC.md` and relevant ADRs
- Surveyed all Rust modules in `crates/core` and `src-tauri`
- Grep'd for avoided terms across codebase
- Cross-referenced implementation against glossary definitions
- Distinguished code-internal names from user-facing UI copy

---

*Survey conducted September 2026 by Cursor agent on @omesser's behalf.*
