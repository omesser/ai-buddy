# Test fixtures

`pi-acp-banner.txt` — a real capture, with the paths replaced. Pi's startup
banner as it arrives on the ACP wire, taken from `pi` 0.85.1 through `pi-acp`
0.0.33 on a fresh session: one `agent_message_chunk` of 1651 bytes, ahead of
anything the model wrote (#609).

The home directory and the skill names in it were a real user's, and this is a
public repository, so they are replaced with neutral ones. Nothing else is:
the version line, the `---` rule, the blank line, the `## Skills` heading and
the count of path lines are the capture's own. Those are what the tests turn
on — the shape of a preamble, not whose files it lists.
