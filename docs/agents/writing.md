# Writing

Voice belongs to the `developer-voice` skill. This file holds the one thing no
skill can know: where writing lands in this repository.

## The pull request title is the line that survives

`main` takes squash merges only. The commit subject is the pull request title
and the body is empty, so nothing else reaches `git log`.

- Write the title as the one-sentence summary a reader should find a year from
  now.
- Branch commit bodies serve the reviewer and the merge discards them. Keep them
  short, or leave them out.
- Reasoning that has to outlive the review goes in a code comment or the pull
  request description. `docs/agents/comments.md` covers the first.
