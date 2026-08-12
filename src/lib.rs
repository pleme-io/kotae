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

#![forbid(unsafe_code)]

use serde::Serialize;
use serde_json::{Map, Value, json};

/// What a tool concluded — the four distinguishable outcomes.
///
/// A closed sum, and the closure is the primitive: a fifth kind of answer has
/// no value, so a server cannot invent one that a reader has no rule for.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The question was answered and there is something. The payload is the
    /// tool's own shape — kotae holds no opinion about it.
    Found(Value),

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

impl Answer {
    /// Something was found. The payload is serialized as-is.
    ///
    /// # Errors
    ///
    /// Never — a payload that cannot be serialized becomes a [`Self::Blind`]
    /// naming the serialization failure, because a tool that found something
    /// and cannot say so has not answered.
    #[must_use]
    pub fn found<T: Serialize>(payload: &T) -> Self {
        match serde_json::to_value(payload) {
            Ok(v) => Self::Found(v),
            Err(e) => Self::Blind {
                because: format!("the result could not be serialized: {e}"),
            },
        }
    }

    /// A pre-built JSON payload.
    #[must_use]
    pub fn found_value(payload: Value) -> Self {
        Self::Found(payload)
    }

    /// Nothing was found, and that is the answer.
    #[must_use]
    pub fn empty(of: impl Into<String>) -> Self {
        Self::Empty { of: of.into() }
    }

    /// The question was not understood, and here is what would be.
    #[must_use]
    pub fn refused(because: impl Into<String>, legal: impl IntoIterator<Item = impl Into<String>>) -> Self {
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

    /// The stable discriminant. Derived from the variant, never chosen at a
    /// call site — which is what makes two different answers unable to render
    /// the same way.
    #[must_use]
    pub fn outcome(&self) -> &'static str {
        match self {
            Self::Found(_) => "found",
            Self::Empty { .. } => "empty",
            Self::Refused { .. } => "refused",
            Self::Blind { .. } => "blind",
        }
    }

    /// Whether this answer says anything about the subject at all.
    ///
    /// `false` for [`Self::Blind`] and [`Self::Refused`]: both are statements
    /// about the *question*, not about the world. A caller aggregating answers
    /// must not count them as evidence.
    #[must_use]
    pub fn is_about_the_world(&self) -> bool {
        matches!(self, Self::Found(_) | Self::Empty { .. })
    }

    /// Render to the JSON string an MCP tool returns.
    ///
    /// Infallible by construction — see the crate docs on why the fallback
    /// cannot itself produce garbage.
    #[must_use]
    pub fn render(&self) -> String {
        let value = self.to_value();
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
            // Unreachable in practice: `to_value` already produced a `Value`,
            // and every `Value` serializes. Kept because "unreachable" and
            // "panics in production" are one refactor apart, and this is the
            // one function that must never fail to say something.
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
            Self::Found(payload) => {
                // The payload is spliced when it is an object, so a tool's own
                // keys stay at the top level and existing consumers of a
                // hand-rolled server keep reading the same shape. A non-object
                // payload (an array, a scalar) is nested under `result`,
                // because an array cannot carry a discriminant.
                match payload {
                    Value::Object(fields) => {
                        let mut out = Map::new();
                        out.insert("outcome".into(), Value::String("found".into()));
                        for (k, v) in fields {
                            // A payload key named `outcome` would overwrite the
                            // discriminant and make a `found` answer able to
                            // claim it was `blind`. The payload loses.
                            if k != "outcome" {
                                out.insert(k.clone(), v.clone());
                            }
                        }
                        Value::Object(out)
                    }
                    other => json!({ "outcome": "found", "result": other }),
                }
            }
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
            Self::Blind { because } => json!({
                "outcome": "blind",
                "because": because,
                "note": "The tool could NOT look. This is not an absence and not \
                         a denial — conclude nothing about the subject.",
            }),
        }
    }
}

impl std::fmt::Display for Answer {
    /// The wire form, so an `Answer` can be returned where a `String` is
    /// expected without a visible conversion at every call site.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

impl From<Answer> for String {
    fn from(a: Answer) -> Self {
        a.render()
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
        assert!(empty["note"].as_str().unwrap().contains("nothing was found"));
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
        let bad: std::collections::HashMap<[u8; 2], i32> =
            [([1, 2], 3)].into_iter().collect();
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
}
