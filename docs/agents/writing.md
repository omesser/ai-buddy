# Writing

Voice belongs to the
[`developer-voice`](https://github.com/omesser/ai-goodies/tree/main/skills/developer-voice)
skill. Read it there if you do not have it loaded. This file holds the one thing
no skill can know: where writing lands in this repository.

## The pull request title is the line that survives

`main` takes squash merges only. The commit subject is the pull request title
and the body is empty, so nothing else reaches `git log`.

- Write the title as the one-sentence summary a reader should find a year from
  now.
- Branch commit bodies serve the reviewer and the merge discards them. Keep them
  short, or leave them out.
- Reasoning that has to outlive the review goes in a code comment or the pull
  request description. `docs/agents/comments.md` covers the first.

That last point overrides `developer-voice`, which puts the reasoning in the
commit body. Here the body does not survive.
