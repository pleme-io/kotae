# kotae (答え)

**The typed answer a machine gives a machine.**

A tool call has four distinguishable outcomes. Most tools render two.

```rust
use kotae::Answer;

Answer::found(&rows)                                   // here it is
Answer::empty("pods in `kube-system`")                 // there is nothing — a finding
Answer::refused("unknown kind `poddz`", ["pod", "node"]) // you asked wrong
Answer::blind("the credential expired")                // I could not look
```

Each renders JSON carrying an explicit `outcome` discriminant, and **no two of
them can produce the same bytes**.

## The typed form

`Answer` is `Outcome<serde_json::Value>`. A library returns `Outcome<P>` with
its own payload type and renders through the same code at the border:

```rust
use kotae::{Outcome, OutcomeKind};

let o: Outcome<Vec<Pod>> = Outcome::Found(pods);
assert_eq!(o.kind(), OutcomeKind::Found);
let wire = o.into_answer();
```

`OutcomeKind::ALL` lists the four arms. The enum and the list come from one
table, so a fifth arm cannot exist unlisted.

## A named authority

`Authority` is a trimmed, non-empty name of who authorized a mutation. The only
constructor is `Authority::parse(Option<&str>)`; a missing or blank value is a
`refused` answer that names the legal form. It has no `Deserialize`: keep the
wire field an `Option<String>` and parse it at the boundary, so one bad entry
is refused alone.

```rust
let who = kotae::Authority::parse(args.authorized_by.as_deref())?;
```

## Why this exists

The reader of a tool's answer is a language model with no peripheral vision. It
cannot glance at a status bar, notice a stack trace scrolling past, or feel that
a response looked odd. It has the bytes and nothing else. So the bytes have to
carry every distinction that matters:

| The tool… | and the reader must conclude… |
|---|---|
| answered, and there is something | here it is |
| answered, and there is nothing | **there is nothing** — a finding |
| did not understand the question | **you asked wrong** — retry differently |
| could not put the question at all | **nobody knows** — not an absence |

Collapse any two and you get a confident wrong answer. The expensive collapse is
the fourth row folding into the second: an empty list means *the deployment is
gone*, and an agent reading a credential expiry as an empty list will report a
workload deleted that is running perfectly.

## The measurement that produced it

Across the pleme-io fleet's five MCP servers, counted 2026-08-12:

| server | `#[tool]` fns | hand-rolled result serializations |
|---|---:|---:|
| mado | 65 | 160 |
| tear | 12 | 25 |
| banken | 13 | 1 |
| frost-mcp | 6 | 9 |
| escriba-mcp | 0 | 2 |
| **total** | **96** | **197** |

Ninety-six tools, a hundred and ninety-seven independent decisions about how to
say "it worked" and "it didn't". banken's ratio is what the others could be.

## The bug it removes

`tear` built its error answers with `format!("{{\"error\":\"{e}\"}}")` in 24
places. An error message containing a double quote — a file path, a quoted
identifier, or a caller-supplied id echoed back — produced invalid JSON:

```text
{"error":"file "a.txt" not found"}
                ^ the object ends here as far as any parser is concerned
```

Several of those sites interpolated agent-supplied text directly, so the
malformed output was reachable from the other end of the protocol.

Every string in `Answer` goes through serde. Typed emission is not a style
preference here; it is the difference between an answer and a parse error.

## Guarantees

- **The discriminant is derived from the variant**, never chosen at a call site,
  so two answers that mean different things cannot render alike.
- **A payload cannot forge the discriminant.** A tool returning its own
  `outcome` field loses to the variant.
- **A refusal carries no data keys** — no `rows`, no `count`, no `result` — so
  it can never be mistaken for an empty success.
- **`render()` is infallible**, and its fallback is a `Value` built from owned
  `String`s rather than a `format!`, because a renderer whose error path can
  itself emit garbage has not solved the problem it claims to.
- **An unenumerable legal set is omitted, not empty.** `[]` reads as "nothing is
  legal", which is a different and alarming claim.

## What it is not

Not an MCP framework, not a transport, not an error type. It holds no opinion
about your payload — `Answer::Found` carries whatever you serialize, and an
object payload keeps its keys at the top level so a server migrating from a
hand-rolled shape does not break its consumers.

There is deliberately no `(def…)` authoring form: an `Answer` is a runtime
value, not an authored declaration, and nobody writes answers in Lisp. The
authorable surface in this domain is a *catalog* of a server's tools and the
answer shapes each can return — a named follow-up, not a shipped form
(`pending-kotae: defkotae-catalog`).

## License

MIT
