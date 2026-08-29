# Writing

The prose rules `docs/agents/comments.md` states for code apply to everything
else a contributor writes: commit messages, pull request descriptions, and
review replies. Every sentence earns its place.

- Lead with the answer. The first sentence says what changed or what is true.
- One idea per sentence. Active voice.
- No filler, no exclamation marks, no metaphor where the literal mechanism will
  do, no stacked em-dash asides, no sentence fragments used as punchlines.

The last one is not theoretical. Pull request #56 opened on this line:

> Not the arithmetic — the window.

The repo owner rejected it. It was built for effect rather than to state a
fact, and a reader who did not already know the bug learned nothing from it.
The replacement says the thing: "The overlay now covers one display and follows
the Character onto whichever display it is on."

## Commit messages

Imperative subject, under 50 characters. The body says why the change exists
and what breaks without it. It does not summarise the diff, because `git show`
already does that, and does it better.

`c73abdb` is the shape. It names the failure it fixes, `curl: (3) nested brace
in URL`, explains that ERE has no lazy quantifier so `.+?` matched a long span
of JSON, and then justifies a decision the diff cannot:

> Pinned for the same reason rust-toolchain.toml is: a formatter that updates
> itself can reformat the whole repository and fail a pull request that changed
> none of it.

None of that is recoverable from the diff.

## Pull request descriptions

A reviewer scans before reading. Lead with the answer, then use sections with
bullets. Use a table where the content compares things.

#52 is the model. One lead sentence, `git add -A` kept offering to commit a git
repository inside this one, then three short sections: why, why the whole
directory rather than a list, and the two commands that verify it. #57 does the
same and puts its six before-and-after line counts in a table, because that is
what the content is.

## Review replies

Say what changed and why. Name what you did not change and why not.

The third comment on #56 answers a suggestion by rejecting it, with the
measurement that settles it:

> `cover_display` is asynchronous: the move is queued rather than applied
> inside the tick. So the frame being skipped was still correct [...] The
> `continue` therefore drops a good frame and fixes nothing. Reverted.

A reviewer whose suggestion is silently dropped has to ask again.

## Completeness beats brevity

This is not a length limit. A change a reviewer must be warned about earns
whatever length that warning takes.

#56 is long on purpose. It records a first diagnosis that was wrong, a written
specification decision it supersedes in four places, and a known limitation
where the sprite is clipped on the seam between two displays. Cut any of the
three and the pull request gets shorter and the review gets worse.

The rule is that every sentence earns its place, not that there are few of them.
