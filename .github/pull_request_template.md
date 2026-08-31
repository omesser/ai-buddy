<!--
Title: <type>[optional scope]: <description>

  feat  fix  docs  test  refactor  perf  ci  build  chore  revert  style

`main` takes squash merges, so this title becomes the commit subject and is the
only line that reaches `git log`. Write the summary a reader should find a year
from now. Append `!` after the type for a breaking change: `feat(engine)!: ...`

See docs/agents/writing.md.
-->

## Why

<!-- The problem, before the solution. What is wrong today, or what cannot be
     done yet? A reader who knows nothing about this work starts here. -->

## What changed

<!-- The shape of the change, not a diff summary. Name the decisions a reviewer
     would otherwise have to reverse-engineer, and say what you deliberately
     did not do. -->

## How to verify

<!-- What you ran, and what a reviewer can run. For a behavior change, the
     evidence that it works: test names, mutation results, harness output, or
     the manual steps if it can only be checked by hand. -->

## Notes

<!-- Optional. Risks, follow-ups, merge-order dependencies on other pull
     requests, anything you could not verify and why. Delete if empty. -->

Closes #
