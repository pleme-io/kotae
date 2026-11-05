use serde::{Serialize, Serializer};

use crate::Answer;

/// A named authority for a mutation: a trimmed, non-empty name of the human or
/// agent that authorized a write.
///
/// The field is private and the only constructor is [`Authority::parse`], so
/// an empty or whitespace-only authority has no value. A gate that takes an
/// `Authority` cannot be handed an unattributed write.
///
/// There is deliberately no `Deserialize`. A wire field stays an
/// `Option<String>` and is parsed at the boundary, where a blank value is
/// refused for that one request. Failing deserialization instead would refuse
/// the whole document it sits in, and every valid sibling with it.
///
/// ```
/// use kotae::Authority;
///
/// let who = Authority::parse(Some("  alice ")).unwrap();
/// assert_eq!(who.as_str(), "alice");
///
/// let refused = Authority::parse(Some("   ")).unwrap_err();
/// assert_eq!(refused.outcome(), "refused");
/// ```
///
/// The name cannot be built or reached around the parser:
///
/// ```compile_fail
/// let forged = kotae::Authority(String::new());
/// ```
///
/// ```compile_fail
/// let who = kotae::Authority::parse(Some("alice")).unwrap();
/// let inner: &String = &who.0;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Authority(String);

impl Authority {
    /// The one legal form, as the refusal names it.
    pub const LEGAL: &'static str =
        "a non-blank name of the human or agent that authorized this mutation";

    /// Parse a boundary value into an authority.
    ///
    /// # Errors
    ///
    /// A missing, empty or whitespace-only value is refused, and the refusal
    /// names [`Self::LEGAL`].
    pub fn parse(raw: Option<&str>) -> Result<Self, Answer> {
        match raw.map(str::trim) {
            Some(who) if !who.is_empty() => Ok(Self(who.to_owned())),
            Some(_) => Err(Answer::refused(
                "a mutation needs a named authority, and the one given was blank",
                [Self::LEGAL],
            )),
            None => Err(Answer::refused(
                "a mutation needs a named authority, and none was given",
                [Self::LEGAL],
            )),
        }
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name, owned.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for Authority {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Authority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Authority {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutcomeKind;

    #[test]
    fn a_missing_empty_or_whitespace_authority_is_refused_with_its_legal_row() {
        for raw in [None, Some(""), Some(" "), Some("\t\n  \r")] {
            let refused = Authority::parse(raw).unwrap_err();
            assert_eq!(refused.kind(), OutcomeKind::Refused, "{raw:?}");
            assert!(!refused.is_about_the_world(), "{raw:?}");
            match refused {
                Answer::Refused { legal, .. } => {
                    assert_eq!(legal, vec![Authority::LEGAL.to_owned()], "{raw:?}");
                }
                other => panic!("{raw:?} produced {other:?}"),
            }
        }
    }

    #[test]
    fn absent_and_blank_say_different_things() {
        let absent = Authority::parse(None).unwrap_err();
        let blank = Authority::parse(Some("  ")).unwrap_err();
        assert_ne!(absent, blank);
    }

    #[test]
    fn a_named_authority_is_trimmed_and_kept() {
        let who = Authority::parse(Some("  alice@ops \n")).unwrap();
        assert_eq!(who.as_str(), "alice@ops");
        assert_eq!(who.to_string(), "alice@ops");
        assert_eq!(who.as_ref(), "alice@ops");
        assert_eq!(who.clone().into_string(), "alice@ops");
    }

    #[test]
    fn inner_whitespace_is_part_of_the_name() {
        let who = Authority::parse(Some(" Ada Lovelace ")).unwrap();
        assert_eq!(who.as_str(), "Ada Lovelace");
    }

    #[test]
    fn an_authority_serializes_as_its_bare_name() {
        let who = Authority::parse(Some("alice")).unwrap();
        assert_eq!(
            serde_json::to_value(&who).unwrap(),
            serde_json::json!("alice")
        );
    }

    #[test]
    fn the_refusal_renders_as_a_refused_answer_naming_the_legal_form() {
        let v = Authority::parse(None).unwrap_err().to_value();
        assert_eq!(v["outcome"], "refused");
        assert_eq!(v["legal"][0], Authority::LEGAL);
    }
}
