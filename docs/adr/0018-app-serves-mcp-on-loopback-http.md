# The app serves MCP on loopback HTTP, and the stdio binary is the fallback

The tools are dispatched inside the running app, on the frame-loop thread, and
reached over `http://127.0.0.1:<ephemeral>/mcp` behind a per-run bearer token.
`session/new` hands that URL and header to any Harness whose `initialize`
advertised `mcpCapabilities.http`. Every other Harness gets the stdio server
#497 made reachable three ways — `AI_BUDDY_MCP_BIN`, an `ai-buddy-mcp` sidecar
beside the app, then the app binary re-executed as `ai-buddy --mcp-stdio` — in
that precedence, unchanged. A session with no MCP server at all is no longer an
outcome; #497 removed it.

The reason is not the transport. It is where a tool can be dispatched at all.
`speak` resolves a target against the Instance list and enqueues a
`BehaviorProposal` on the `Roster`, which is the `ExpressionHandle`; the
`Roster` is owned by the frame loop and has no representation outside the
process. So a separate process cannot serve these tools, only pretend to —
which is exactly what it did. `ai-buddy-mcp` built its `DispatchContext` from
`StubWindowSource`, an empty roster and no expression handle, and
`tools::speak` reports an empty roster as success. #16 was closed on a promise
to fix this, #433 attached a real Harness without fixing it, and every
verification since stopped at "the tool call returned success" (#470, #166).

## Considered Options

Both of these are #166's own framing of the choice, and it is a choice between
two ways to reach one endpoint, because either way the endpoint has to be
inside the app.

- **`ai-buddy-mcp` becomes a thin shim that dials the running app** over a
  local socket, a named pipe, or loopback. Every Harness is covered, including
  the ones with no HTTP MCP, and the copy-paste snippet stays one command.
  It costs the endpoint *plus* the shim, plus a discovery file the shim reads
  to find the port and the token — a fourth thing to keep secret and to keep in
  step with a running app. Not rejected: deferred, and now #501, which is the
  follow-up that covers `hermes`. #497's `--mcp-stdio` route is where it will
  land, since that is already the app's own binary and needs no new one.
- **The app serves MCP itself on loopback HTTP** and the binary shrinks to a
  fallback. Chosen. The endpoint alone, with no shim, no discovery file and no
  second process: ACP already carries a URL and headers in
  `session/new`'s `mcpServers`, so the Harness needs told nothing else.
  ADR-0017 already records `mcpCapabilities.http` off the handshake for this.
- **A TCP MCP transport**, which #166's body originally called the
  prerequisite for Grok. Dropped there before this: none of the five Harnesses
  has a TCP client config, and the one that was said to want it reaches HTTP
  today with `--transport http`.

`std::net::TcpListener` and about two hundred lines, rather than `rmcp`'s
streamable-HTTP server or axum. The MCP spec lets a server answer a POST with
`application/json` instead of an SSE stream, and nothing here needs a
server-initiated message: seven synchronous tools, no sampling, no progress,
no resources. What that buys is worth naming, because the usual answer is a web
framework — a desktop app's dependency tree does not gain hyper, tower and a
second tokio runtime for one endpoint that answers one client.

## Security

The endpoint is a listener this process owns, which is a shape ADR-0010's
credential rules did not have in view — they are about credentials belonging to
someone else. They read the same way for one we mint, and #470 asks for it
explicitly.

1. **Loopback only.** The listener binds `127.0.0.1` on an ephemeral port, and
   a connection whose peer is not loopback is dropped before a byte is read.
   Nothing is reachable from another machine, and no port is fixed for another
   program to squat.
2. **A per-run token.** Thirty-two bytes from the OS CSPRNG, minted at startup,
   held in memory for the life of the process. It is never written to
   `settings.json`, `harness-session.json`, the Action Log, or a trace: the
   Action Log's `attach` event and the probe's `mcp` line both record
   `McpChoice::label`, which is the URL. `Endpoint` implements neither `Debug`
   nor `Serialize` so that a token cannot reach a log line through a format
   string, and hands out a header rather than the token itself.
3. **A request carrying an `Origin` header is refused.** No MCP client sends
   one and a page in the user's own browser must, so refusing it closes the
   DNS-rebinding hole the MCP spec warns about without a guess at which origins
   are the user's.
4. **The token travels in a header**, not in the URL and not in an argv. It is
   in neither a config file the Harness keeps nor a process list.

The Harness still authenticates itself and ai-buddy still holds no credential
of the Harness's. Nothing here is one of theirs.

## Consequences

**The stdio fallback still dispatches through stubs, and a reader must not
take #497's three routes for three working ones.** All three run the same
`crates/mcp-server` code with `StubWindowSource`, an empty roster and no
expression handle: Memory is real, sensing sees nothing, and `speak` and
`play_behavior` reach no Instance while reporting success. So a `hermes`
session — or any Harness reached over stdio, the app binary's own
`--mcp-stdio` included — is told the line was said and nothing appears on
screen. **#470 is closed for the loopback HTTP path only.** #501 is the shim
that would dial the running app and close it for stdio too; #502 is the
success `tools::speak` reports for an empty roster, which is what makes the
lie silent. `crates/mcp-server/src/lib.rs`'s header says all of this at the
code.

Saying it rather than deleting `StubWindowSource` is the honest half of #470's
fourth acceptance line: the type is what a process outside the app can have,
since platform sensing lives behind the Shell's own consent (ADR-0005).

A `tools/call` is answered on the frame loop, one channel hop and at most one
tick away, and dispatch is synchronous there. That is a tool call inside the
frame budget's own thread — acceptable because a tool call is rare and
`dispatch` does no network I/O, and it is the same trade the chat drain beside
it already makes. A call that finds no frame loop is refused after five
seconds rather than hanging a turn. `ADR-0004` is untouched: no future reaches
the loop.

Two Instances and a `speak` naming neither is a refusal, not a guess, because
`resolve_target_instance` calls it ambiguous. That is the roster's answer and
now a Harness can actually receive it — `list_instances` is what makes the
refusal answerable.

Reversing this means either #501's shim (which needs the endpoint anyway) or
giving the `Roster` a representation outside the process, which is a second
copy of the thing the frame loop owns.
