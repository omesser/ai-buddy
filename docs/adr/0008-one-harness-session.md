# One Harness session is the Director, chat, and everything else

A second model for idle life is a second mind. The HTTP Completer that #11
ships is a stand-in until a Harness is attached. After that, Director
proposals, chat, and any later tool use share that Harness's conversation —
one session, not a cheap classifier plus Claude Code.

Static weights stay the Director when nothing is attached, when the Director
is off, and when a session call fails. They cost nothing, so they may stay
chatty. A session wake spends tokens, so it is not a heartbeat.

## Considered Options

- **Standalone inference API forever.** An OpenAI-compatible Completer for
  idle Behaviors, a Harness for chat. Cheap ambient life without installing
  an agent — and two voices, two memories of what just happened. Rejected.
- **Harness is the only model, including before attach.** No Completer at
  all. Correct end state; leaves #11 with nothing to measure against Static
  until #16 lands. The HTTP path stays as a disposable Completer behind the
  same `Director` trait.
- **Fixed interval (two minutes, or a settings slider).** Predictable, and
  a bill for scratching. Rejected for any path that reaches a model.

## Wake policy

A session wake is either **reactive** — the user addressed the buddy (Poke,
Throw, picking it up, placing it on a Perch, Summon, a chat turn) — or
**proactive**, on an exponential backoff the Character Manifest names
(`model_base` and `model_power` under `[director]`), paused
while every display is asleep. The Engine and Static Director keep the
sprite moving between those calls. ADR-0004 still holds: the model is never
in the frame loop.

## Consequences

#16 does not grow a second Director. The attached Harness *is* the
Completer. Chat (#17) is another turn in the same conversation, not a
different prompt to a different endpoint.

Reversing this means living with split-brain, or rewriting how #15–#17
talk to a model.
