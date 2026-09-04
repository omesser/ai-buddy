# The Instance Prompt is the user's authored layer of the Character Prompt

The Character Prompt gains a second authored layer. The Character Package says
who the Character is; the user's own text says who this Character Instance is.
The chat surface (#17) gets a tab showing both: the Character's
`personality.txt` frozen and read-only for reference, and beneath it the
**Instance Prompt** — per-Instance text, empty by default, that the user writes.

It sits after the Personality Prompt, as a second voice layer, bound by the same
rule: demeanour only, never capability. In `character_prompt` it is appended to
`context.personality`, still ahead of the declared-Behavior roster and the
universal voice rules, so those rules come last and govern both authored layers
by position as well as by wording.

The tab shows the two authored layers and nothing else. The assembled payload
stays inspectable in settings, where `docs/SPEC.md:376` already puts it. One
inspect surface, not two.

## The vocabulary this contradicts

`CONTEXT.md` said the Character Prompt is "never authored by hand". That clause
is now false and is amended: the Character Prompt is *assembled* and never
hand-authored as a whole, and two of its layers are authored. An `Instance
Prompt` entry is added beside it.

`CONTEXT.md` also said Instances "differ in personality and behavior, never in
what they know". This layer is what that clause was always describing, not a
second thing beside it. Until now the clause was a promise with no mechanism:
two Instances of one Character were identical in voice, differing only in name
and position. Its second half is untouched — Memory stays shared, so Instances
still never differ in what they know.

## Editing it reopens the session

The Character Prompt is the *opening turn*. `ModelDirector` sends it once —
`opened` latches on the first successful wake, later wakes send `follow_up`, and
the `Endpoint` holds the message list. A user editing mid-session has changed
text the live conversation opened without, and no follow-up can retrofit it.

**On save, the session is torn down and reopened.** `retarget_model` is both the
precedent and the mechanism: `InFlight::cancel` drops a call on the wire so the
old host stops generating, `in_flight` clears, and the next wake opens fresh.
`switch_instance` already does this for a Character switch, for the same reason.

So saving is a real act with a visible cost, and the surface must say so rather
than imply a live edit: **saving starts a new conversation.** The model-side
transcript of the chat is gone, including anything in it never written to
Memory. That is the price of the edit taking effect at all, and Memory — not the
session — is where anything worth keeping lives.

- The trigger is an explicit save, never a keystroke. Per-keystroke
  invalidation would wipe the conversation mid-sentence.
- A reply in flight is cancelled, not awaited; the thinking indicator stops.
- The text takes effect at the *next* wake, not retroactively. Nothing is
  re-asked to prove the change landed; the buddy's next line is the new one.
- The change lands in the Action Log (#16). A prompt layer changing is exactly
  what a user needs to find later.

Deferring to "the next natural opening" was rejected: that opening may be hours
away or may never come, and a user who typed a paragraph and saw nothing change
would reasonably conclude the feature is broken.

## The capability bound holds structurally

User story 49 applies unchanged to the user's own layer, which is more trusted
than a package's and still cannot mint a Behavior the Character never declared,
promise action on the machine, or override the universal voice rules.

None of that relies on the prompt asking nicely. The Engine plays only declared
Behaviors and `ModelDirector::declared` refuses a name nobody declared, so an
invented Behavior is a near miss rather than an action; ai-buddy ships no
Executor (ADR-0003), so a promised action has nothing to call. The wording only
saves a wasted round trip.

The text takes the same 2000-character bound as `PERSONALITY_LIMIT`, for the
reason spelled out there: untrusted text in every opening turn, so an unbounded
one spends the user's tokens and buries the sensing context under prose. Two
authored layers double that worst case, which is the number to revisit if either
bound moves.

## The text follows the Instance, and survives a Character switch

It is per-Instance state and stays with the Instance when the user switches that
Instance to another Character: text written for a BMO Instance is kept when that
Instance becomes Timber Wolf.

Dropping it is silent loss of the user's own words for a reversible act, and
switching back would then have to resurrect what was discarded. The nonsense
risk is bounded and self-correcting: the text is demeanour-only either way, the
switch already reopens the session, and the tab shows the retained text beside
the new Character's frozen personality at exactly the moment it may have become
wrong. Visible and editable beats deleted.

The implementation pays for this: an Instance's id is stable only within a run.
`Roster::spawn` mints a fresh `uuid` and `remember_instances` persists
`InstanceSpec { character, name }` with no id, so keying the text by today's id
loses it on every restart. `InstanceSpec` needs a persisted id first.

## What it is not

Not a system prompt — the Character Prompt is the payload, this is one layer
inside it. Not a jailbreak surface for the model behind the Completer: text
aiming at that model's own limits is unsupported, not a feature. Not a place to
paste a Harness's own instructions, which belong in the Harness's own session
(ADR-0003, ADR-0008) — pasting them here confuses demeanour with a tool
contract. Not a place for facts about the user; that is Memory.

## Consequences

`character_prompt` takes a second authored string, saving the tab cancels and
reopens that Instance's Director session, `InstanceSpec` grows a persisted id,
and the bound on the opening turn doubles.

Reversing this means deleting the tab. The layer is additive and empty by
default, so an Instance without one assembles exactly the Character Prompt it
does today.
