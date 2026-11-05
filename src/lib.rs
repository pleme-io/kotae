//! kotae (答え) — the typed answer a machine gives a machine.
//!
//! # The problem this domain names
//!
//! A tool answers a question. Across the fleet's five MCP servers there are
//! **96 `#[tool]` functions and 197 hand-rolled result serializations**
//! (measured 2026-08-12: mado 65/160, tear 12/25, banken 13/1, frost-mcp 6/9,
//! escriba-mcp 0/2). Every one of them decides, freshly and by hand, how to
//! turn a result into a string — and the decisions disagree.
//!
//! The disagreement is not cosmetic. A *reader* of these answers is a language
//! model with no peripheral vision: it cannot glance at a status bar, notice a
//! stack trace scrolling past, or feel that a response looked odd. It has the
//! bytes and nothing else. So the bytes have to carry every distinction that
//! matters, and there are four:
//!
//! | The tool… | and the reader must conclude… |
//! |---|---|
//! | answered, and there is something | here it is |
//! | answered, and there is nothing | **there is nothing** — a finding |
//! | did not understand the question | **you asked wrong** — retry differently |
//! | could not put the question at all | **nobody knows** — not an absence |
//!
//! Collapse any two and you get a confident wrong answer. The expensive one is
//! the third row folding into the second: an empty list means *the deployment
//! is gone*, and an agent that reads a credential expiry as an empty list will
//! report a workload deleted that is running perfectly.
//!
//! # What this crate is
//!
//! One closed sum — [`Answer`] — with four arms, and a renderer that gives
//! each arm a **structurally distinct** JSON shape carrying an explicit
//! `outcome` discriminant. Two answers that mean different things cannot
//! render the same way, because the discriminant is derived from the variant
//! rather than chosen at the call site.
//!
//! ```
//! use kotae::Answer;
//!
//! // A finding: the namespace really is empty.
//! let empty = Answer::empty("pods in `kube-system`");
//! assert!(empty.render().contains(r#""outcome": "empty""#));
//!
//! // NOT a finding: we could not look.
//! let blind = Answer::blind("the credential expired");
//! assert!(blind.render().contains(r#""outcome": "blind""#));
//!
//! // They are different bytes, which is the whole point.
//! assert_ne!(empty.render(), blind.render());
//! ```
//!
//! # Why not `format!()`
//!
//! Because it is already broken in production. `tear` builds its error answers
//! with `format!("{{\"error\":\"{e}\"}}")` in **17 places**, and an error
//! message containing a double quote — a file path, a quoted identifier,
//! anything a parser echoes back — produces invalid JSON:
//!
//! ```text
//! {"error":"file "a.txt" not found"}
//!                 ^ the object ends here as far as any parser is concerned
//! ```
//!
//! Every string in [`Answer`] goes through serde. ★★ TYPED EMISSION is not a
//! style preference here; it is the difference between an answer and a parse
//! error.
//!
//! # The fallback cannot fail
//!
//! Serialization of an arbitrary payload *can* fail (a map with non-string
//! keys, a float that is NaN). [`Answer::render`] is infallible anyway, and
//! the fallback is not a `format!()` — it is a [`serde_json::Value`] built
//! from owned `String`s, whose serialization is total because every leaf is
//! already a valid JSON scalar. A renderer whose error path can itself emit
//! garbage has not solved the problem it claims to.
//!
//! # No `(def…)` form, and that is deliberate
//!
//! An [`Answer`] is a **runtime value**, not an authored declaration. Nobody
//! writes answers in Lisp; a tool produces them from what it found. Bolting a
//! `#[derive(DeriveTataraDomain)]` on would buy an authoring surface for
//! something nobody authors, and drag a parser into every MCP server's
//! dependency tree for it.
//!
//! There *is* an authorable surface in this domain — a catalog declaring each
//! tool and the answer shapes it can return, which would let a server's
//! contract be checked rather than described. That is a named follow-up, not a
//! shipped form: `pending-kotae: defkotae-catalog`.
//!
//! # The typed form, and the wire form
//!
//! [`Outcome<P>`] is the four arms generic over the found payload, so a
//! library returns its own shape and a caller branches on a type rather than
//! on a string. [`Answer`] is `Outcome<Value>`: the same sum, with the payload
//! already serialized, and the one every renderer reads. Any `Outcome<P>` with
//! a serializable payload renders through the same code, so the typed and the
//! wire form cannot disagree about a discriminant.
//!
//! ```
//! use kotae::{Answer, Outcome, OutcomeKind};
//!
//! let typed: Outcome<Vec<u32>> = Outcome::Found(vec![1, 2]);
//! assert_eq!(typed.kind(), OutcomeKind::Found);
//! let wire: Answer = typed.into_answer();
//! assert_eq!(wire.outcome(), "found");
//! ```
//!
//! [`OutcomeKind::ALL`] enumerates the arms. It and the enum are generated
//! from one table, so a fifth arm cannot exist without being listed.
//!
//! # A named authority for a mutation
//!
//! [`Authority`] is a trimmed, non-empty name of who authorized a write. The
//! only way to hold one is [`Authority::parse`], whose refusal is an
//! [`Answer`] naming what would be accepted, so a blank authority is refused at
//! the boundary and never reaches the act it would have authorized.

#![forbid(unsafe_code)]

mod authority;

pub use authority::Authority;

use serde::Serialize;
use serde_json::{Map, Value, json};

macro_rules! outcome_kinds {
    ($($(#[$doc:meta])* $variant:ident => $wire:literal, about_the_world: $world:literal;)+) => {
        /// Which of the four arms an [`Outcome`] is, with no payload.
        ///
        /// Generated with [`OutcomeKind::ALL`] from one table, so the list of
        /// arms and the enum cannot drift apart.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum OutcomeKind {
            $($(#[$doc])* $variant,)+
        }

        impl OutcomeKind {
            /// Every arm, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The stable wire discriminant a reader keys on.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }

            /// Whether an answer of this arm says anything about the subject.
            #[must_use]
            pub const fn is_about_the_world(self) -> bool {
                match self {
                    $(Self::$variant => $world,)+
                }
            }
        }
    };
}

outcome_kinds! {
    /// Answered, and there is something.
    Found => "found", about_the_world: true;
    /// Answered, and there is nothing: a finding.
    Empty => "empty", about_the_world: true;
    /// The question was not understood.
    Refused => "refused", about_the_world: false;
    /// The question could not be put at all.
    Blind => "blind", about_the_world: false;
}

impl std::fmt::Display for OutcomeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a tool concluded — the four distinguishable outcomes, generic over the
/// found payload.
///
/// A closed sum, and the closure is the primitive: a fifth kind of answer has
/// no value, so a server cannot invent one that a reader has no rule for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<P> {
    /// The question was answered and there is something. The payload is the
    /// tool's own shape — kotae holds no opinion about it.
    Found(P),

    /// The question was answered and there is **nothing**.
    ///
    /// A finding, not a failure. `of` names what was looked for, so the answer
    /// reads as "no pods in kube-system" rather than a bare `[]` the reader has
    /// to reconstruct the subject of.
    Empty {
        /// What was looked for, in the tool's own words.
        of: String,
    },

    /// The question was not understood.
    ///
    /// `legal` names what *would* have been understood. Naming it is the
    /// difference between a refusal that costs one retry and one that costs a
    /// conversation — and an agent given no legal set will guess.
    Refused {
        /// Why the question could not be taken as asked.
        because: String,
        /// The values that would have been accepted. Empty when the legal set
        /// is genuinely not enumerable; prefer naming it whenever it is.
        legal: Vec<String>,
    },

    /// The question could not be put at all.
    ///
    /// **Not an absence and not a denial.** The network was down, the
    /// credential expired, the daemon was not running. The reader must not
    /// conclude anything about the subject — only about the attempt.
    Blind {
        /// What stopped the question being asked.
        because: String,
    },
}

/// The wire answer: an [`Outcome`] whose payload is already JSON.
pub type Answer = Outcome<Value>;

impl<P> Outcome<P> {
    /// Nothing was found, and that is the answer.
    #[must_use]
    pub fn empty(of: impl Into<String>) -> Self {
        Self::Empty { of: of.into() }
    }

    /// The question was not understood, and here is what would be.
    #[must_use]
    pub fn refused(
        because: impl Into<String>,
        legal: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Refused {
            because: because.into(),
            legal: legal.into_iter().map(Into::into).collect(),
        }
    }

    /// The question was not understood and the legal set is not enumerable.
    ///
    /// The weaker form. Prefer [`Self::refused`] whenever the alternatives can
    /// be listed — a refusal that cannot teach makes the reader guess, and a
    /// guessing reader retries wrong.
    #[must_use]
    pub fn refused_flatly(because: impl Into<String>) -> Self {
        Self::Refused {
            because: because.into(),
            legal: Vec::new(),
        }
    }

    /// The question could not be put.
    #[must_use]
    pub fn blind(because: impl Into<String>) -> Self {
        Self::Blind {
            because: because.into(),
        }
    }

    /// Which arm this is.
    #[must_use]
    pub const fn kind(&self) -> OutcomeKind {
        match self {
            Self::Found(_) => OutcomeKind::Found,
            Self::Empty { .. } => OutcomeKind::Empty,
            Self::Refused { .. } => OutcomeKind::Refused,
            Self::Blind { .. } => OutcomeKind::Blind,
        }
    }

    /// The stable discriminant. Derived from the variant, never chosen at a
    /// call site — which is what makes two different answers unable to render
    /// the same way.
    #[must_use]
    pub const fn outcome(&self) -> &'static str {
        self.kind().as_str()
    }

    /// Whether this answer says anything about the subject at all.
    ///
    /// `false` for [`Self::Blind`] and [`Self::Refused`]: both are statements
    /// about the *question*, not about the world. A caller aggregating answers
    /// must not count them as evidence.
    #[must_use]
    pub const fn is_about_the_world(&self) -> bool {
        self.kind().is_about_the_world()
    }

    /// Map the found payload, leaving the other three arms untouched.
    pub fn map<Q>(self, f: impl FnOnce(P) -> Q) -> Outcome<Q> {
        match self {
            Self::Found(p) => Outcome::Found(f(p)),
            Self::Empty { of } => Outcome::Empty { of },
            Self::Refused { because, legal } => Outcome::Refused { because, legal },
            Self::Blind { because } => Outcome::Blind { because },
        }
    }
}

impl Answer {
    /// Something was found. The payload is serialized as-is.
    ///
    /// # Errors
    ///
    /// Never — a payload that cannot be serialized becomes a [`Self::Blind`]
    /// naming the serialization failure, because a tool that found something
    /// and cannot say so has not answered.
    #[must_use]
    pub fn found<T: Serialize + ?Sized>(payload: &T) -> Self {
        match serde_json::to_value(payload) {
            Ok(v) => Self::Found(v),
            Err(e) => Self::Blind {
                because: unserializable(&e),
            },
        }
    }

    /// A pre-built JSON payload.
    #[must_use]
    pub fn found_value(payload: Value) -> Self {
        Self::Found(payload)
    }
}

fn unserializable(e: &serde_json::Error) -> String {
    format!("the result could not be serialized: {e}")
}

impl<P: Serialize> Outcome<P> {
    /// The wire answer for this outcome, serializing the found payload.
    ///
    /// A payload that cannot be serialized becomes [`Outcome::Blind`], exactly
    /// as [`Answer::found`] does.
    #[must_use]
    pub fn to_answer(&self) -> Answer {
        match self {
            Self::Found(p) => Answer::found(p),
            Self::Empty { of } => Outcome::Empty { of: of.clone() },
            Self::Refused { because, legal } => Outcome::Refused {
                because: because.clone(),
                legal: legal.clone(),
            },
            Self::Blind { because } => Outcome::Blind {
                because: because.clone(),
            },
        }
    }

    /// The wire answer for this outcome, consuming it.
    #[must_use]
    pub fn into_answer(self) -> Answer {
        match self {
            Self::Found(p) => Answer::found(&p),
            Self::Empty { of } => Outcome::Empty { of },
            Self::Refused { because, legal } => Outcome::Refused { because, legal },
            Self::Blind { because } => Outcome::Blind { because },
        }
    }

    /// Render to the JSON string an MCP tool returns.
    ///
    /// Infallible by construction — see the crate docs on why the fallback
    /// cannot itself produce garbage.
    #[must_use]
    pub fn render(&self) -> String {
        let value = self.to_value();
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
            let mut fallback = Map::new();
            fallback.insert("outcome".into(), Value::String("blind".into()));
            fallback.insert(
                "because".into(),
                Value::String("the answer could not be rendered".into()),
            );
            Value::Object(fallback).to_string()
        })
    }

    /// The structured form, for a caller that composes rather than returns.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Found(p) => match serde_json::to_value(p) {
                Ok(payload) => found_wire(payload),
                Err(e) => blind_wire(&unserializable(&e)),
            },
            Self::Empty { of } => json!({
                "outcome": "empty",
                "of": of,
                "note": "This IS the answer: nothing was found. The tool looked \
                         and there is nothing there.",
            }),
            Self::Refused { because, legal } => {
                let mut out = Map::new();
                out.insert("outcome".into(), Value::String("refused".into()));
                out.insert("because".into(), Value::String(because.clone()));
                if !legal.is_empty() {
                    out.insert(
                        "legal".into(),
                        Value::Array(legal.iter().cloned().map(Value::String).collect()),
                    );
                }
                out.insert(
                    "note".into(),
                    Value::String(
                        "The QUESTION was not understood. This says nothing about \
                         whether the thing you asked for exists."
                            .into(),
                    ),
                );
                Value::Object(out)
            }
            Self::Blind { because } => blind_wire(because),
        }
    }
}

/// The `found` wire shape: an object payload keeps its keys at the top level,
/// minus any `outcome` key, which would forge the discriminant. A non-object
/// payload is nested under `result`, because an array cannot carry a
/// discriminant.
fn found_wire(payload: Value) -> Value {
    match payload {
        Value::Object(fields) => {
            let mut out = Map::new();
            out.insert("outcome".into(), Value::String("found".into()));
            out.extend(fields.into_iter().filter(|(k, _)| k != "outcome"));
            Value::Object(out)
        }
        other => json!({ "outcome": "found", "result": other }),
    }
}

fn blind_wire(because: &str) -> Value {
    json!({
        "outcome": "blind",
        "because": because,
        "note": "The tool could NOT look. This is not an absence and not \
                 a denial — conclude nothing about the subject.",
    })
}

impl<P: Serialize> std::fmt::Display for Outcome<P> {
    /// The wire form, so an answer can be returned where a `String` is
    /// expected without a visible conversion at every call site.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

impl<P: Serialize> From<Outcome<P>> for String {
    fn from(o: Outcome<P>) -> Self {
        o.render()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **THE INVARIANT THE CRATE EXISTS FOR.** Two answers that mean different
    /// things must be distinguishable *by the field a reader keys on* — not
    /// merely by having different bytes somewhere.
    ///
    /// The distinction is not pedantic, and a red run proved it: collapsing
    /// `Blind` into `outcome: "empty"` left the two rendering different bytes
    /// (one carries `because`, the other `of`), so a whole-string `assert_ne!`
    /// stayed green while the exact confusion this crate exists to prevent was
    /// live. A reader keying on `outcome` would have read "we could not look"
    /// as "there is nothing there".
    ///
    /// So this asserts on the RENDERED discriminant — read back out of the
    /// serialized JSON, not from `outcome()`, because the renderer is what a
    /// consumer actually sees and the two could drift.
    #[test]
    fn no_two_outcomes_are_confusable_by_a_reader() {
        let answers = [
            Answer::found_value(json!({ "rows": [] })),
            Answer::empty("pods"),
            Answer::refused("unknown kind `poddz`", ["pod", "service"]),
            Answer::blind("the credential expired"),
        ];
        let rendered: Vec<(String, &Answer)> = answers
            .iter()
            .map(|a| {
                let v: Value = serde_json::from_str(&a.render()).expect("valid JSON");
                (
                    v["outcome"]
                        .as_str()
                        .expect("every answer renders a discriminant")
                        .to_owned(),
                    a,
                )
            })
            .collect();

        for (i, (da, a)) in rendered.iter().enumerate() {
            for (db, b) in &rendered[i + 1..] {
                assert_ne!(
                    da, db,
                    "{a:?} and {b:?} render the SAME discriminant `{da}` — a \
                     reader keying on `outcome` cannot tell them apart",
                );
            }
            // And the rendered discriminant must agree with the type's own,
            // or a caller branching on `outcome()` and a reader branching on
            // the JSON would disagree.
            assert_eq!(da, a.outcome(), "{a:?}: rendered vs typed discriminant");
        }

        // `refused_flatly` is the same outcome as `refused` on purpose — both
        // are "the question was not understood" — so it is checked for
        // agreement rather than for distinctness.
        assert_eq!(Answer::refused_flatly("x").outcome(), "refused");
    }

    /// **The expensive confusion, pinned.** An empty result and a failed look
    /// must be distinguishable by a reader keying on one field — not by
    /// reading prose.
    #[test]
    fn empty_and_blind_are_distinguishable_by_the_discriminant_alone() {
        let empty = Answer::empty("pods in kube-system").to_value();
        let blind = Answer::blind("the apiserver was unreachable").to_value();
        assert_eq!(empty["outcome"], "empty");
        assert_eq!(blind["outcome"], "blind");
        assert_ne!(empty["outcome"], blind["outcome"]);
        // And each says in words what it means, for a reader that does not
        // know the vocabulary yet.
        assert!(
            empty["note"]
                .as_str()
                .unwrap()
                .contains("nothing was found")
        );
        assert!(blind["note"].as_str().unwrap().contains("not an absence"));
    }

    /// **A refusal never carries data keys.** A `refused` answer that also had
    /// `rows` would let a reader treat it as an empty success — which is
    /// exactly the collapse this type prevents.
    #[test]
    fn a_refusal_carries_no_data_keys() {
        let v = Answer::refused("unknown kind `poddz`", ["pod", "node"]).to_value();
        assert_eq!(v["outcome"], "refused");
        assert!(v["rows"].is_null());
        assert!(v["result"].is_null());
        assert!(v["count"].is_null());
        assert_eq!(v["legal"][0], "pod");
    }

    /// **The tear bug, made impossible.** An error message containing a double
    /// quote produced invalid JSON when string-formatted. Here it round-trips.
    #[test]
    fn a_quote_in_the_message_does_not_break_the_json() {
        let nasty = r#"file "a.txt" not found: {"unbalanced": ["#;
        for answer in [
            Answer::blind(nasty),
            Answer::refused_flatly(nasty),
            Answer::empty(nasty),
        ] {
            let rendered = answer.render();
            let parsed: Value =
                serde_json::from_str(&rendered).expect("every answer must be valid JSON");
            assert_eq!(parsed["outcome"], answer.outcome());
        }
    }

    /// A newline, a tab, a NUL and a lone astral char must survive too — the
    /// same class as the quote, and the one a hand-rolled escaper forgets.
    #[test]
    fn control_characters_and_astral_planes_round_trip() {
        let nasty = "line\none\ttwo\u{0}\u{1F9CA} \\ backslash";
        let rendered = Answer::blind(nasty).render();
        let parsed: Value = serde_json::from_str(&rendered).expect("valid JSON");
        assert_eq!(parsed["because"], nasty);
    }

    /// A `found` object payload keeps its keys at the top level, so a server
    /// migrating from a hand-rolled shape does not break its consumers.
    #[test]
    fn a_found_object_payload_keeps_its_own_keys() {
        let v = Answer::found_value(json!({ "count": 2, "rows": [1, 2] })).to_value();
        assert_eq!(v["outcome"], "found");
        assert_eq!(v["count"], 2);
        assert_eq!(v["rows"][1], 2);
    }

    /// **The payload may not overwrite the discriminant.** A tool returning a
    /// field called `outcome` would otherwise be able to make a `found` answer
    /// claim it was `blind`, which is the one field a reader trusts.
    #[test]
    fn a_payload_cannot_forge_the_discriminant() {
        let v = Answer::found_value(json!({ "outcome": "blind", "rows": [] })).to_value();
        assert_eq!(v["outcome"], "found", "the payload must not win");
        assert_eq!(v["rows"], json!([]));
    }

    /// A non-object payload cannot carry a discriminant, so it is nested
    /// rather than silently dropped.
    #[test]
    fn a_non_object_payload_is_nested_under_result() {
        let v = Answer::found_value(json!([1, 2, 3])).to_value();
        assert_eq!(v["outcome"], "found");
        assert_eq!(v["result"], json!([1, 2, 3]));
    }

    /// `found` of an unserializable payload becomes `blind`, not a panic and
    /// not a silent empty — a tool that found something and cannot say so has
    /// not answered.
    #[test]
    fn an_unserializable_payload_becomes_blind() {
        // A map with non-string keys is the canonical serde_json failure.
        let bad: std::collections::HashMap<[u8; 2], i32> = [([1, 2], 3)].into_iter().collect();
        let a = Answer::found(&bad);
        assert_eq!(a.outcome(), "blind");
        let parsed: Value = serde_json::from_str(&a.render()).expect("valid JSON");
        assert!(
            parsed["because"].as_str().unwrap().contains("serialized"),
            "{parsed}",
        );
    }

    /// `Refused` and `Blind` are statements about the QUESTION. A caller
    /// aggregating answers must not count them as evidence about the world.
    #[test]
    fn only_found_and_empty_are_about_the_world() {
        assert!(Answer::found_value(json!({})).is_about_the_world());
        assert!(Answer::empty("pods").is_about_the_world());
        assert!(!Answer::refused_flatly("bad").is_about_the_world());
        assert!(!Answer::blind("down").is_about_the_world());
    }

    /// An empty legal set is omitted rather than rendered as `[]` — an empty
    /// array reads as "nothing is legal", which is a different and alarming
    /// claim from "the legal set is not enumerable".
    #[test]
    fn an_unenumerable_legal_set_is_omitted_not_empty() {
        let v = Answer::refused_flatly("malformed").to_value();
        assert!(v["legal"].is_null(), "{v}");
        let w = Answer::refused("bad", ["a"]).to_value();
        assert_eq!(w["legal"], json!(["a"]));
    }

    /// Every answer renders as valid JSON carrying its own discriminant — the
    /// property every other test depends on.
    #[test]
    fn every_variant_renders_parseable_json_with_its_discriminant() {
        for a in [
            Answer::found_value(json!({ "x": 1 })),
            Answer::empty("x"),
            Answer::refused("x", ["y"]),
            Answer::blind("x"),
        ] {
            let parsed: Value = serde_json::from_str(&a.render()).expect("valid JSON");
            assert_eq!(parsed["outcome"], a.outcome());
        }
    }

    #[test]
    fn display_and_into_string_agree_with_render() {
        let a = Answer::empty("pods");
        assert_eq!(a.to_string(), a.render());
        assert_eq!(String::from(a.clone()), a.render());
    }

    /// The all-variants row: one sample per arm, its wire string and whether
    /// it is about the world. The match has no wildcard, so a new arm does not
    /// compile until it has a row here.
    fn row(kind: OutcomeKind) -> (Outcome<Vec<u32>>, &'static str, bool) {
        match kind {
            OutcomeKind::Found => (Outcome::Found(vec![7]), "found", true),
            OutcomeKind::Empty => (Outcome::empty("pods"), "empty", true),
            OutcomeKind::Refused => (Outcome::refused("bad kind", ["pod"]), "refused", false),
            OutcomeKind::Blind => (Outcome::blind("vpn down"), "blind", false),
        }
    }

    #[test]
    fn every_arm_has_a_row_and_every_row_agrees_with_every_surface() {
        assert_eq!(OutcomeKind::ALL.len(), 4, "a new arm needs a row in `row`");
        for &kind in OutcomeKind::ALL {
            let (typed, wire, world) = row(kind);
            assert_eq!(typed.kind(), kind);
            assert_eq!(kind.as_str(), wire);
            assert_eq!(kind.to_string(), wire);
            assert_eq!(typed.outcome(), wire);
            assert_eq!(kind.is_about_the_world(), world);
            assert_eq!(typed.is_about_the_world(), world);

            let parsed: Value = serde_json::from_str(&typed.render()).expect("valid JSON");
            assert_eq!(parsed["outcome"], wire, "{kind:?}: rendered discriminant");

            let answer = typed.to_answer();
            assert_eq!(answer.kind(), kind);
            assert_eq!(answer.render(), typed.render(), "{kind:?}: typed vs wire");
            assert_eq!(typed.clone().into_answer(), answer);
            assert_eq!(String::from(typed.clone()), typed.render());
            assert_eq!(typed.map(|v| v.len()).kind(), kind);
        }
    }

    #[test]
    fn all_lists_each_arm_once_with_distinct_wire_strings() {
        let mut seen = std::collections::HashSet::new();
        for &kind in OutcomeKind::ALL {
            assert!(
                seen.insert(kind.as_str()),
                "{kind:?} repeats a discriminant"
            );
        }
        assert_eq!(seen.len(), OutcomeKind::ALL.len());
    }

    #[test]
    fn map_touches_only_the_found_payload() {
        assert_eq!(Outcome::Found(2).map(|n| n * 10), Outcome::Found(20));
        assert_eq!(
            Outcome::<u8>::empty("x").map(u32::from),
            Outcome::<u32>::empty("x")
        );
        assert_eq!(
            Outcome::<u8>::refused("r", ["a"]).map(u32::from),
            Outcome::<u32>::refused("r", ["a"])
        );
        assert_eq!(
            Outcome::<u8>::blind("b").map(u32::from),
            Outcome::<u32>::blind("b")
        );
    }

    #[test]
    fn a_typed_unserializable_payload_renders_blind() {
        let bad: std::collections::HashMap<[u8; 2], i32> = [([1, 2], 3)].into_iter().collect();
        let typed = Outcome::Found(bad);
        assert_eq!(typed.kind(), OutcomeKind::Found);
        let parsed: Value = serde_json::from_str(&typed.render()).expect("valid JSON");
        assert_eq!(parsed["outcome"], "blind");
        assert_eq!(typed.to_answer().kind(), OutcomeKind::Blind);
        assert_eq!(typed.into_answer().kind(), OutcomeKind::Blind);
    }

    #[test]
    fn a_typed_object_payload_cannot_forge_the_discriminant() {
        #[derive(Serialize)]
        struct Forged {
            outcome: &'static str,
            rows: u8,
        }
        let v = Outcome::Found(Forged {
            outcome: "blind",
            rows: 1,
        })
        .to_value();
        assert_eq!(v["outcome"], "found");
        assert_eq!(v["rows"], 1);
    }
}
