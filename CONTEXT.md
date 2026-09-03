# ai-buddy

A desktop companion in the spirit of Windows 95-era desktop mascots: an animated
sprite that lives on your screen, reacts to the windows around it, and can be
asked to do real work on your machine.

## Language

### The character

**Character**:
The shippable unit a user installs and chooses between — identity, art,
personality, and tuning bundled together.
_Avoid_: Pet, mascot, avatar, buddy (the app is the buddy, not the character)

**Character Package**:
The on-disk form of a Character: a directory or archive containing its
animations, Character Manifest, personality prompt, and behavior tuning.
_Avoid_: Skin, theme, mod, plugin

**Character Manifest**:
The declaration at the root of a Character Package: the frames, fps and loop
mode of each Animation, the Behaviors the Character declares, and how
proactive model calls space themselves. Frame size and frame count are read
from the art rather than declared.
_Avoid_: Manifest on its own — a Memory Manifest is one too

**Personality Prompt**:
The natural-language description of who a Character is, carried in its package
and given to the Director. Governs demeanour only, never capability.
_Avoid_: System prompt — that is the Character Prompt, which carries this and more

**Character Prompt**:
The opening turn of the Director session: Personality Prompt, the Behaviors
it may propose, and this moment. Later wakes send a short follow-up (what
just happened, recent Behaviors, time of day, State, frontmost window) in
the same conversation. Inspectable in settings, never authored by hand.
_Avoid_: Persona, preamble, prompt template

**Animation**:
A named frame sequence belonging to a Character. Pure art with no logic.
_Avoid_: Clip, sprite, sequence

**Required Animation Set**:
The animations every Character Package must supply for the engine to drive it.
_Avoid_: Base set, defaults

**Character Instance**:
One spawned buddy: a Character plus a user-given name and a stable id. Instances
differ in personality and behavior, never in what they know.
_Avoid_: Session, spawn, copy, clone

**Memory**:
The single durable record of what the buddies know about the user. Shared by
every Character Instance, and owned by the user: readable, editable in any text
editor, and wipeable.
_Avoid_: History, context, knowledge base, store, profile

**Memory Manifest**:
The on-disk form of Memory: one Markdown file of facts under stable headings,
which the user may read and edit by hand.
_Avoid_: Manifest on its own — a Character Package has one too

### Life on screen

**State**:
Where the sprite is anchored and which physics apply — grounded, falling,
dragged, perched, climbing, asleep.
_Avoid_: Mode, status, pose

**Primitive**:
An engine-owned unit of motion or expression that Behaviors are composed from.
Characters may compose Primitives but never define new ones.
_Avoid_: Action, command, step

**Behavior**:
A named sequence of Primitives with weights and trigger conditions, declared as
data in a Character Package. The unit the Director proposes and the engine plays.
_Avoid_: Routine, script, macro

**Director**:
The role that proposes a Behavior, and Speech when the session is on. Static
weights fill Behaviors and never speak; an attached Harness is that role and
proposes Speech by calling speak. Never runs in the frame loop and never
drives animation directly.
_Avoid_: Brain, agent, planner

**Proactive model call**:
A Director session wake that fires because the buddy was left alone long
enough, not because the user addressed it.
_Avoid_: Unused model call, unused wake, active prompting

**Perch**:
A window's top edge treated as a one-way platform the sprite can land on, walk
along, and fall off. Window sides and bottoms are not Perches, and neither is
the length of an edge that cannot be seen: one hidden behind a window in front
of it, hanging over no display, or so close to the usable top that the art
would sit behind the menu bar. That governs landing and staying: an unseen
edge is gone, and the sprite falls. A yank past the ride gate drops it too.
A slow drag is still the same edge: the sprite Holds and rides.
_Avoid_: Ledge, platform, surface

**Hold**:
The Primitive and required Animation of gripping a moving Perch so the sprite
keeps its place on the edge. Engine-played, like Land: no Director proposes it
in time. Not a State — the sprite stays Perched.
_Avoid_: Squat, cling, grab (Grab is the verb that picks the sprite up)

**Talk**:
The Primitive and Required Animation of a talking mouth. Art, not words —
Speech may play it; a silent reaction may too.
_Avoid_: speak

**Surface**:
What the sprite stands on: a display's floor, or a Perch. The umbrella over
both, not a synonym for Perch — a Perch is one kind of Surface, and "surface"
as a loose word for a Perch stays on that entry's avoid list.
_Avoid_: Ground, platform

**Contact**:
What one tick of physics reports back to the State machine: the sprite landed
on a Surface, was lifted onto one, stands where it stood, hangs in the air, or
met a wall or the ceiling. An observation only — what the sprite becomes as a
result is the State machine's decision, never the Contact's.
_Avoid_: Collision, hit, event

### Layers

**Spatial Layer**:
The always-on, local, model-free system: physics, window geometry, Behaviors,
and the interaction verbs. Works offline with no permissions granted.
_Avoid_: Idle mode, pet mode

**Functional Layer**:
The invoked system that performs real work on the machine through an attached
Harness. Asynchronous, explicitly Summoned, and reported on by the Spatial Layer.
_Avoid_: Agent mode, assistant mode, copilot

**Harness**:
An external agent runtime the user attaches, which reasons and acts on their
behalf. Supplied by the user, never bundled.
_Avoid_: Backend, provider, model

**Completer**:
An HTTP chat-completions endpoint standing in for a Harness until one is
attached, behind the same session trait
([ADR-0008](./docs/adr/0008-one-harness-session.md)). Settings names its timeout
and reply cap, which is where the word reaches the screen.
_Avoid_: Model, LLM, provider, API

**Executor**:
Whatever posts synthetic mouse and keyboard events to the operating system.
Owned by the Harness or a desktop-control MCP server, not by ai-buddy.
_Avoid_: Driver, automation layer, robot

**Action Log**:
The readable record of what the Functional Layer did and why: the Character
Prompts sent, the answers returned, and the actions the Harness took. Points at
the Harness's own session dump rather than copying it.
_Avoid_: Memory log, transcript, audit trail

### Sensing

**Ambient Capture**:
Low-frequency, consented sampling of the screen that runs while the user works.
_Avoid_: Monitoring, watching, background scan

**On-Demand Capture**:
A single capture taken in direct response to a user act — a Poke, a call, a
chat message.
_Avoid_: Manual capture, triggered scan

**Local Gate**:
The mandatory on-device filter every Capture passes through before anything may
reach the Director. Discards unchanged and uninteresting frames.
_Avoid_: Preprocessor, filter, throttle

### Interaction verbs

**Grab**:
Press and move — the sprite follows the cursor.

**Throw**:
Release a Grab with velocity — the sprite travels ballistically until it lands.

**Poke**:
A click on the sprite — provokes a reaction and possibly a line of dialogue.

**Menu**:
Right-click on the sprite — character switching, settings, quit.

**Summon**:
The deliberate act that opens the Functional Layer.
_Avoid_: Invoke, activate, wake

### Expression

**Speech**:
The line the buddy says. The session Director proposes it on a wake — Static
never speaks — and an attached Harness proposes it by calling speak.
_Avoid_: talk (the Required Animation), message, utterance

**speak**:
The MCP tool by which a Harness proposes Speech. Until then the session
Director proposes the same Speech without this tool.
_Avoid_: talk, say

**Speech bubble**:
A bubble above the sprite showing Speech, held for reading time (900ms
+ 55ms per character, clamped to 2–8 s). A new line replaces the old one.
Implemented in #119.
_Avoid_: Chat bubble, message, tooltip

**Cue**:
The Shell's acknowledgement that one interaction landed: a procedural visual
over the sprite and a synthesized sound, one pair per interaction — Poke,
Summon, Menu, pickup, drop, and a throw that is the drop played harder. The
Engine names the Cue on the frame; the webview draws and synthesizes it, so no
Character declares one. Do Not Disturb silences the sound and keeps the visual.
A machine that cannot start an audio context does the same. #277, #292.
_Avoid_: Effect, feedback, animation (the Character's art), SFX

**Thinking ellipsis**:
Three animated dots in a bubble above the sprite, shown while a reactive
Director turn is in flight (Poke, Summon, Throw). Appears after 250ms grace,
held ≥600ms once shown. Proactive wakes stay invisible. #119.
_Avoid_: Loading, spinner, progress
