// SPDX-FileCopyrightText: The flint contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Case-style formats (`{{ name|camel }}`, `{{ name|pascal }}`, ...) for
//! `{{ variable }}` placeholders, and the canonicalization used to decide
//! whether two differently-cased occurrences of the same variable name
//! represent the same underlying identifier.
//!
//! There is no case-conversion crate dependency here (`heck`,
//! `convert_case`, ...) — those resolve acronym runs (`XML`, `ID`, ...) and
//! digits (`file2`) by picking one grouping and discarding the
//! alternatives, which is exactly
//! the information this module can't afford to lose: two independently
//! written spellings of the same identifier need to be recognized as such
//! without knowing which grouping the author intended. See
//! [`groupings`]'s doc comment for the algorithm.

/// One of the ten case-style filters a `{{ name|format }}` placeholder can
/// request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Format {
    /// `camelCase`.
    Camel,
    /// `PascalCase`.
    Pascal,
    /// `snake_case`.
    Snake,
    /// `SCREAMING_SNAKE_CASE`.
    ScreamingSnake,
    /// `kebab-case`.
    Kebab,
    /// `SCREAMING-KEBAB-CASE`.
    ScreamingKebab,
    /// `Title Case`.
    Title,
    /// `Train-Case`.
    Train,
    /// `dot.case`.
    Dot,
    /// `SCREAMING.DOT.CASE`.
    ScreamingDot,
}

/// Single source of truth for filter-name <-> [`Format`] in both
/// directions, so [`Format::parse`] and [`Format::filter_name`] can never
/// drift apart.
const FILTER_NAMES: [(&str, Format); 10] = [
    ("camel", Format::Camel),
    ("pascal", Format::Pascal),
    ("snake", Format::Snake),
    ("screamingSnake", Format::ScreamingSnake),
    ("kebab", Format::Kebab),
    ("screamingKebab", Format::ScreamingKebab),
    ("title", Format::Title),
    ("train", Format::Train),
    ("dot", Format::Dot),
    ("screamingDot", Format::ScreamingDot),
];

impl Format {
    /// Looks up the [`Format`] named by the text after `|` in a
    /// `{{ name|format }}` placeholder (e.g. `"camel"`). Returns `None` for
    /// anything else, including an empty string.
    pub(crate) fn parse(name: &str) -> Option<Format> {
        FILTER_NAMES
            .iter()
            .find(|(filter_name, _)| *filter_name == name)
            .map(|(_, format)| *format)
    }

    /// The filter name this format is written as in `.flint` syntax, e.g.
    /// `Format::ScreamingSnake.filter_name() == "screamingSnake"`.
    pub(crate) fn filter_name(self) -> &'static str {
        FILTER_NAMES
            .iter()
            .find(|(_, format)| *format == self)
            .map(|(filter_name, _)| *filter_name)
            .expect("every Format variant has a FILTER_NAMES entry")
    }

    /// Every filter name [`Self::parse`] accepts, in the order the docs
    /// list them — what an "unknown format" error offers instead.
    pub(crate) fn filter_names() -> [&'static str; FILTER_NAMES.len()] {
        FILTER_NAMES.map(|(filter_name, _)| filter_name)
    }

    /// The non-capturing regex fragment `template.rs` embeds inside its
    /// named capture group instead of the unconstrained `[^/]+`, so a
    /// directory entry that doesn't fit this format's shape simply fails to
    /// match the surrounding template line. This is the only place a
    /// format's shape is decided: [`groupings`] reads whatever was
    /// captured, without checking it again.
    ///
    /// A name is made of letters, digits and combining marks of any
    /// script, and emoji. What a format restricts is their case, and a
    /// character with no case — a digit, an emoji, or a letter of a script
    /// that has none, like `語` — suits any of them.
    pub(crate) fn regex_fragment(self) -> String {
        self.fragment(&ANY_SCRIPT)
    }

    /// [`Self::regex_fragment`] for a name known to be ASCII, which this
    /// accepts or rejects just the same. It's a far smaller regex, and
    /// so far quicker to compile.
    pub(crate) fn ascii_regex_fragment(self) -> String {
        self.fragment(&ASCII)
    }

    /// The regex fragment for this format's shape, written in `classes`
    /// — the shared body of [`Self::regex_fragment`] and
    /// [`Self::ascii_regex_fragment`].
    fn fragment(self, classes: &Classes) -> String {
        let (separator, case) = match self {
            Format::Camel => return format!("{}[{}]*", classes.not_upper, classes.word),
            Format::Pascal => (None, TokenCase::Title),
            Format::Snake => (Some('_'), TokenCase::Lower),
            Format::ScreamingSnake => (Some('_'), TokenCase::Upper),
            Format::Kebab => (Some('-'), TokenCase::Lower),
            Format::ScreamingKebab => (Some('-'), TokenCase::Upper),
            Format::Title => (Some(' '), TokenCase::Title),
            Format::Train => (Some('-'), TokenCase::Title),
            Format::Dot => (Some('.'), TokenCase::Lower),
            Format::ScreamingDot => (Some('.'), TokenCase::Upper),
        };
        separated_regex_fragment(separator, case, classes)
    }
}

/// The regex character classes a format's shape is written in, for one
/// set of characters.
struct Classes {
    /// The characters a name is made of, as the inside of a class.
    word: &'static str,
    /// One of them that isn't a capital.
    not_upper: &'static str,
    /// One of them that isn't a lowercase letter.
    not_lower: &'static str,
}

/// Letters, digits and combining marks, in any script, and emoji. A
/// combining mark is an accent stored as a character of its own, after
/// the letter it belongs to (`x̄` is `x` and a macron) — [`crate::run`]
/// composes each pair that has a single-character form, but not every
/// pair has one. An emoji is often several characters too (`👍🏽` is a
/// thumbs-up and a skin tone, and a family is its members with joiners
/// between them), so the characters that only ever build one count
/// (`Emoji_Component`). Unicode also calls `#`, `*` and the ASCII digits
/// emoji, for the keycaps they can be drawn as, hence the `--\p{ASCII}`.
const ANY_SCRIPT: Classes = Classes {
    word: r"\p{Alphabetic}\p{N}\p{M}[[\p{Emoji}\p{Emoji_Component}]--\p{ASCII}]",
    not_upper: r"[[\p{Alphabetic}\p{N}\p{M}[[\p{Emoji}\p{Emoji_Component}]--\p{ASCII}]]--\p{Uppercase}]",
    not_lower: r"[[\p{Alphabetic}\p{N}\p{M}[[\p{Emoji}\p{Emoji_Component}]--\p{ASCII}]]--\p{Lowercase}]",
};

/// Exactly the ASCII characters in [`ANY_SCRIPT`]'s classes.
const ASCII: Classes = Classes {
    word: "A-Za-z0-9",
    not_upper: "[a-z0-9]",
    not_lower: "[A-Z0-9]",
};

/// The per-token case rule a separator-based format's tokens must satisfy:
/// no capitals (`Lower`) for `snake`/`kebab`/`dot`, no lowercase letters
/// (`Upper`) for `screamingSnake`/`screamingKebab`/`screamingDot`, and not
/// starting with a lowercase letter (`Title`) for `title`/`train`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenCase {
    /// No capitals.
    Lower,
    /// No lowercase letters.
    Upper,
    /// Not starting with a lowercase letter.
    Title,
}

/// The regex fragment for one separator-based format — or, with no
/// `separator`, for a single token — used by [`Format::fragment`].
fn separated_regex_fragment(separator: Option<char>, case: TokenCase, classes: &Classes) -> String {
    let token = match case {
        TokenCase::Lower => format!("{}+", classes.not_upper),
        TokenCase::Upper => format!("{}+", classes.not_lower),
        TokenCase::Title => format!("{}[{}]*", classes.not_lower, classes.word),
    };
    match separator {
        Some(separator) => {
            let sep = regex::escape(&separator.to_string());
            format!("{token}(?:{sep}{token})*")
        }
        None => token,
    }
}

/// Whether there's a word break between two neighbouring characters of a
/// captured value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gap {
    /// Always a word break: a separator, or a capital after a lowercase
    /// letter or a digit in camel/pascal.
    Break,
    /// Never a word break: e.g. inside a lowercase run.
    Join,
    /// Either — case-only text can't tell. In camel/pascal: between two
    /// capitals (`XML` is one word or three), or before a character with
    /// no case, like a digit (`file2` is one word or two, `12` one number
    /// or two). In title/train: before a capital partway through a token
    /// (`YouTube` is one word or two).
    Either,
}

/// Every word-grouping one captured value admits, held compactly: the
/// value's characters lowercased with any separators dropped, plus whether
/// each gap between neighbouring characters is a word break. The groupings
/// are every way of choosing, independently at each [`Gap::Either`] gap,
/// whether to break there — so `parseXMLFile` (camel) admits
/// `[parse, xml, file]`, `[parse, x, m, l, file]`, and every partial
/// grouping in between.
///
/// Gaps being independent is what keeps this cheap: two readings share a
/// grouping exactly when their characters agree and no gap is a
/// [`Gap::Break`] in one and a [`Gap::Join`] in the other, which
/// [`Groupings::narrow`] checks in one pass, rather than listing the 2^n
/// groupings a value with *n* ambiguous gaps admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Groupings {
    /// The value's characters, lowercased (see [`lower`]), without
    /// separators.
    chars: Vec<char>,
    /// `gaps[i]` is the gap between `chars[i]` and `chars[i + 1]`.
    gaps: Vec<Gap>,
    /// Whether this is an unformatted value, which is never recased, so
    /// only agrees with exactly the same text.
    opaque: bool,
}

impl Groupings {
    /// The reading of an unformatted value: one opaque word, so two
    /// readings of this kind share a grouping only when they're the same
    /// text.
    pub(crate) fn opaque(value: &str) -> Groupings {
        let chars: Vec<char> = value.chars().collect();
        let gaps = vec![Gap::Join; chars.len().saturating_sub(1)];
        Groupings {
            chars,
            gaps,
            opaque: true,
        }
    }

    /// Whether `self` and `other` are the same characters. Two formatted
    /// values are compared case-folded (see [`fold`]), since lowercasing
    /// alone leaves a few letters in two forms.
    fn same_chars(&self, other: &Groupings) -> bool {
        if self.chars == other.chars {
            return true;
        }
        !self.opaque
            && !other.opaque
            && self.chars.len() == other.chars.len()
            && self
                .chars
                .iter()
                .zip(&other.chars)
                .all(|(&ours, &theirs)| fold(ours) == fold(theirs))
    }

    /// The groupings both `self` and `other` admit, or `None` if they
    /// share none.
    pub(crate) fn narrow(&self, other: &Groupings) -> Option<Groupings> {
        if !self.same_chars(other) {
            return None;
        }
        let gaps = self
            .gaps
            .iter()
            .zip(&other.gaps)
            .map(|(&ours, &theirs)| match (ours, theirs) {
                (Gap::Either, gap) | (gap, Gap::Either) => Some(gap),
                (ours, theirs) if ours == theirs => Some(ours),
                _ => None,
            })
            .collect::<Option<Vec<Gap>>>()?;
        Some(Groupings {
            chars: self.chars.clone(),
            gaps,
            opaque: self.opaque,
        })
    }

    /// The one grouping with a word break at every [`Gap::Break`] and
    /// [`Gap::Either`].
    fn words(&self) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        for (index, &c) in self.chars.iter().enumerate() {
            current.push(c);
            let split = match self.gaps.get(index) {
                // `None` is the end of the value, which ends its last word.
                Some(Gap::Break | Gap::Either) | None => true,
                Some(Gap::Join) => false,
            };
            if split {
                words.push(std::mem::take(&mut current));
            }
        }
        words
    }

    /// Whether this value admits exactly the grouping `words`.
    #[cfg(test)]
    fn admits(&self, words: &[&str]) -> bool {
        let chars: Vec<char> = words.concat().chars().collect();
        let mut gaps = Vec::new();
        for word in words {
            gaps.extend(std::iter::repeat_n(Gap::Join, word.chars().count() - 1));
            gaps.push(Gap::Break);
        }
        gaps.pop();
        let grouping = Groupings {
            chars,
            gaps,
            opaque: false,
        };
        self.narrow(&grouping).is_some()
    }
}

/// Every word-grouping `raw` admits under `format`. `raw` is expected to
/// fit that format's shape, having been captured by
/// [`Format::regex_fragment`] — nothing here checks it does, so that the
/// regex is the only judge of that, and a value it accepts can always be
/// read.
///
/// Some word breaks can't be told from case alone. In camel/pascal, a run
/// of capitals carries no signal distinguishing "an intentional acronym"
/// from "a run of single-letter words" (`XML` vs `X`+`M`+`L` look identical
/// from inside the run), and a character with no case — a digit, or a
/// letter of a script that has none — may start a new word or just be
/// part of one, since no capital can mark it (`file2` is `file`+`2`, or
/// one word). The
/// separator-based formats have no such ambiguity — a separator is a word
/// break, and nothing else is — except that a capital partway through a
/// `title`/`train` token might be one (`YouTube`). Resolving any of these
/// with a hardcoded dictionary would be incomplete, while resolving it by remembering whichever occurrence
/// parses first would make correctness depend on line order — so instead
/// every grouping is admitted, and two occurrences are the same identifier
/// if they admit at least one grouping in common (see [`Groupings`]).
///
/// This can never make two identifiers that differ in actual *letters*
/// falsely match: every grouping is of `raw`'s own characters, so a real
/// difference in characters (a dropped or swapped letter, or a missing
/// accent) can never produce a shared grouping across two different raw
/// strings — only *regrouping* of identical characters is reconciled.
/// Identical, that is, once case is set aside: see [`fold`] for the few
/// letters where that takes more than lowercasing.
pub(crate) fn groupings(raw: &str, format: Format) -> Groupings {
    match format {
        Format::Camel | Format::Pascal => camel_pascal_groupings(raw),
        Format::Snake | Format::ScreamingSnake => separated_groupings(raw, '_', false),
        Format::Kebab | Format::ScreamingKebab => separated_groupings(raw, '-', false),
        Format::Title => separated_groupings(raw, ' ', true),
        Format::Train => separated_groupings(raw, '-', true),
        Format::Dot | Format::ScreamingDot => separated_groupings(raw, '.', false),
    }
}

/// [`groupings`] for `Format::Camel` and `Format::Pascal`, which differ
/// only in the case of the first letter. A capital after anything but a
/// capital starts a new word, and a lowercase letter continues the word
/// before it (`parse2files` is one word, `parse2Files` at least two).
/// Between two neighbouring capitals, or before a character with no case,
/// there may or may not be a word break: a word starting with one is
/// spelled with nothing to mark it (`1-2-3` in kebab is `123` here), so
/// ruling a break out would leave such a name unable to be read back.
fn camel_pascal_groupings(raw: &str) -> Groupings {
    let chars: Vec<char> = raw.chars().collect();
    let gaps = chars
        .iter()
        .zip(chars.iter().skip(1))
        .map(|(&before, &after)| {
            if after.is_uppercase() {
                if before.is_uppercase() {
                    Gap::Either
                } else {
                    Gap::Break
                }
            } else if !has_case(after) {
                Gap::Either
            } else {
                Gap::Join
            }
        })
        .collect();
    Groupings {
        chars: chars.into_iter().map(lower).collect(),
        gaps,
        opaque: false,
    }
}

/// [`groupings`] for the eight separator-based formats: `raw` splits on
/// `separator` into tokens, each of which is one word — a separator is
/// what marks a new word in these formats, so a digit with none beside it
/// is part of the word (`file2`), and a word of its own would have been
/// written with one (`file_2`). The one exception is a capital partway
/// through a `title`/`train` token (`title_case`), which may or may not
/// start a new word: `YouTube` is one word or `You`+`Tube`, and
/// `Parse2Files` is one word or `Parse2`+`Files`.
fn separated_groupings(raw: &str, separator: char, title_case: bool) -> Groupings {
    let mut chars = Vec::new();
    let mut gaps = Vec::new();
    for token in raw.split(separator) {
        for (index, c) in token.chars().enumerate() {
            if index > 0 {
                gaps.push(if title_case && c.is_uppercase() {
                    Gap::Either
                } else {
                    Gap::Join
                });
            } else if !chars.is_empty() {
                gaps.push(Gap::Break);
            }
            chars.push(lower(c));
        }
    }
    Groupings {
        chars,
        gaps,
        opaque: false,
    }
}

/// Whether `c` is a capital or a lowercase letter, rather than a digit, a
/// combining mark or a letter of a script with neither.
fn has_case(c: char) -> bool {
    c.is_uppercase() || c.is_lowercase()
}

/// `c` in lowercase, always as one character, so that a value keeps its
/// length whatever its case. Only `İ` (the Turkish capital dotted i) has a
/// longer lowercase form — `i` followed by a combining dot — and is read
/// as plain `i`.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// `c` as a capital, always as one character. A letter whose capital is
/// several characters is left as it is (`ŉ` would be `ʼN`), except `ß`:
/// its usual capital is `SS`, but it has a one-letter one too, `ẞ`.
fn upper(c: char) -> char {
    let mut capitals = c.to_uppercase();
    match (capitals.next(), capitals.next()) {
        (Some(capital), None) => capital,
        _ if c == 'ß' => 'ẞ',
        _ => c,
    }
}

/// `c` with its case set aside, for comparing two formatted values: the
/// lowercase of its capital. For nearly every letter that's just its
/// lowercase, but a few capitals have two lowercase forms, which this
/// makes one — Greek `Σ` (`σ`, and `ς` at the end of a word) and `I`
/// (`i`, and Turkish dotless `ı`).
fn fold(c: char) -> char {
    lower(upper(c))
}

/// The single, deterministic word-grouping used to *render* `raw` under a
/// different format, as opposed to [`groupings`], which admits every
/// grouping an occurrence could plausibly mean so the consistency checker
/// can match permissively. Rendering needs exactly one answer. For
/// camel/pascal, which can't say where a break is wanted, a run of capitals
/// (`XML`) is one word per letter, and a digit run after a letter is a word
/// of its own (`parseFile2` is `parse`, `file`, `2`) unless lowercase
/// letters carry on after it, which make it part of their word
/// (`parse2files` stays one word). Any other character with no case
/// carries on the word before it (`25` and `日本語` are one word each). A separator-based format already says:
/// each segment between separators is one word, as written (`file2` stays
/// `file2`, and `YouTube` under `title` stays `youtube`). Unformatted (`format` is `None`) treats the whole value as one
/// opaque word, since there's no format-implied boundary to split it on.
pub(crate) fn canonical_words(raw: &str, format: Option<Format>) -> Vec<String> {
    let Some(format) = format else {
        return vec![raw.to_string()];
    };
    let mut groupings = groupings(raw, format);
    let separated = !matches!(format, Format::Camel | Format::Pascal);
    #[expect(
        clippy::indexing_slicing,
        reason = "gap `i` lies between chars `i` and `i + 1`, so every index here is in bounds"
    )]
    for index in 0..groupings.gaps.len() {
        if groupings.gaps[index] != Gap::Either {
            continue;
        }
        if separated {
            groupings.gaps[index] = Gap::Join;
        } else if has_case(groupings.chars[index + 1]) {
            // Between two capitals: left as a word break.
        } else if groupings.chars[index + 1].is_numeric() && !groupings.chars[index].is_numeric() {
            // Join a digit run to the letters before it unless a capital
            // follows it.
            let run_end =
                (index + 1..groupings.chars.len()).find(|&end| !groupings.chars[end].is_numeric());
            if run_end.is_some_and(|end| groupings.gaps[end - 1] != Gap::Break) {
                groupings.gaps[index] = Gap::Join;
            }
        } else {
            groupings.gaps[index] = Gap::Join;
        }
    }
    groupings.words()
}

/// The inverse of [`canonical_words`]: spells `words` out under `format`.
/// Each word is re-cased fresh from scratch — title-cased (only its first
/// character capitalized) for
/// camel/pascal/title/train, lower/upper-cased for the rest — so a word's
/// casing in whatever format it was *sourced* from is never carried into a
/// *different* target format; e.g. words canonicalized from a
/// `|snake`-sourced `xml_parser` render as `XmlParser` under `|pascal`,
/// not `XMLParser`. Unformatted (`format` is `None`) expects exactly one
/// word — the untouched original value, exactly what [`canonical_words`]
/// produces for `None` — and returns it verbatim.
pub(crate) fn render_words(words: &[String], format: Option<Format>) -> String {
    let Some(format) = format else {
        return words.concat();
    };
    match format {
        Format::Pascal => words.iter().map(|word| title_case(word, true)).collect(),
        Format::Camel => words
            .iter()
            .enumerate()
            .map(|(index, word)| title_case(word, index != 0))
            .collect(),
        Format::Snake => join_with_case(words, '_', TokenCase::Lower),
        Format::ScreamingSnake => join_with_case(words, '_', TokenCase::Upper),
        Format::Kebab => join_with_case(words, '-', TokenCase::Lower),
        Format::ScreamingKebab => join_with_case(words, '-', TokenCase::Upper),
        Format::Title => join_with_case(words, ' ', TokenCase::Title),
        Format::Train => join_with_case(words, '-', TokenCase::Title),
        Format::Dot => join_with_case(words, '.', TokenCase::Lower),
        Format::ScreamingDot => join_with_case(words, '.', TokenCase::Upper),
    }
}

/// `word` lowercased, except for its first character if `capitalize`.
fn title_case(word: &str, capitalize: bool) -> String {
    word.chars()
        .enumerate()
        .map(|(index, c)| {
            if index == 0 && capitalize {
                upper(c)
            } else {
                lower(c)
            }
        })
        .collect()
}

/// `words` joined by `separator`, each re-cased per `case` — the rendering
/// used by the eight separator-based formats.
fn join_with_case(words: &[String], separator: char, case: TokenCase) -> String {
    words
        .iter()
        .map(|word| match case {
            TokenCase::Lower => word.chars().map(lower).collect(),
            TokenCase::Upper => word.chars().map(upper).collect(),
            TokenCase::Title => title_case(word, true),
        })
        .collect::<Vec<_>>()
        .join(&separator.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`groupings`] of `raw`, if `format`'s regex accepts it — the same
    /// two steps a `.flint` rule takes.
    fn candidates(raw: &str, format: Format) -> Option<Groupings> {
        let regex = regex::Regex::new(&format!(r"\A(?:{})\z", format.regex_fragment())).unwrap();
        regex.is_match(raw).then(|| groupings(raw, format))
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn filter_names_round_trip_for_every_format() {
        for (name, format) in FILTER_NAMES {
            assert_eq!(Format::parse(name), Some(format));
            assert_eq!(format.filter_name(), name);
        }
    }

    /// Fails to compile when a [`Format`] variant is added without being
    /// listed here, so the test below can't quietly skip a new format.
    fn listed_name(format: Format) -> &'static str {
        match format {
            Format::Camel => "camel",
            Format::Pascal => "pascal",
            Format::Snake => "snake",
            Format::ScreamingSnake => "screamingSnake",
            Format::Kebab => "kebab",
            Format::ScreamingKebab => "screamingKebab",
            Format::Title => "title",
            Format::Train => "train",
            Format::Dot => "dot",
            Format::ScreamingDot => "screamingDot",
        }
    }

    #[test]
    fn filter_names_lists_every_format() {
        let expected = [
            Format::Camel,
            Format::Pascal,
            Format::Snake,
            Format::ScreamingSnake,
            Format::Kebab,
            Format::ScreamingKebab,
            Format::Title,
            Format::Train,
            Format::Dot,
            Format::ScreamingDot,
        ]
        .map(listed_name);
        assert_eq!(Format::filter_names(), expected);
    }

    #[test]
    fn unknown_or_empty_filter_name_is_none() {
        assert_eq!(Format::parse("kebob"), None);
        assert_eq!(Format::parse(""), None);
    }

    #[test]
    fn acronym_example_canonicalizes_consistently_across_every_format() {
        let split = ["parse", "x", "m", "l", "file"];
        let grouped = ["parse", "xml", "file"];

        for (raw, format) in [
            ("parseXMLFile", Format::Camel),
            ("ParseXMLFile", Format::Pascal),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&split), "{raw}");
            assert!(reading.admits(&grouped), "{raw}");
            assert!(reading.admits(&["parse", "xm", "l", "file"]), "{raw}");
        }

        for (raw, format) in [
            ("parse_x_m_l_file", Format::Snake),
            ("PARSE_X_M_L_FILE", Format::ScreamingSnake),
            ("parse-x-m-l-file", Format::Kebab),
            ("PARSE-X-M-L-FILE", Format::ScreamingKebab),
            ("Parse X M L File", Format::Title),
            ("Parse-X-M-L-File", Format::Train),
            ("parse.x.m.l.file", Format::Dot),
            ("PARSE.X.M.L.FILE", Format::ScreamingDot),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&split), "{raw}");
            assert!(!reading.admits(&grouped), "{raw}");
        }

        for (raw, format) in [
            ("parse_xml_file", Format::Snake),
            ("PARSE_XML_FILE", Format::ScreamingSnake),
            ("Parse Xml File", Format::Title),
            ("Parse-Xml-File", Format::Train),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&grouped), "{raw}");
            assert!(!reading.admits(&split), "{raw}");
        }

        for (raw, format) in [
            ("Parse XML File", Format::Title),
            ("Parse-XML-File", Format::Train),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&grouped), "{raw}");
            assert!(reading.admits(&split), "{raw}");
        }
    }

    #[test]
    fn number_example_canonicalizes_consistently_across_every_format() {
        for (raw, format) in [
            ("read25Values", Format::Camel),
            ("Read25Values", Format::Pascal),
            ("read_25_values", Format::Snake),
            ("READ_25_VALUES", Format::ScreamingSnake),
            ("read-25-values", Format::Kebab),
            ("READ-25-VALUES", Format::ScreamingKebab),
            ("Read 25 Values", Format::Title),
            ("Read-25-Values", Format::Train),
            ("read.25.values", Format::Dot),
            ("READ.25.VALUES", Format::ScreamingDot),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&["read", "25", "values"]), "{raw}");
        }
    }

    #[test]
    fn camel_and_pascal_digits_after_a_letter_may_or_may_not_start_a_word() {
        for (raw, format) in [
            ("parse2Files", Format::Camel),
            ("Parse2Files", Format::Pascal),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&["parse", "2", "files"]), "{raw}");
            assert!(reading.admits(&["parse2", "files"]), "{raw}");
            // The capital starts a new word.
            assert!(!reading.admits(&["parse2files"]), "{raw}");
            assert!(!reading.admits(&["parse", "2files"]), "{raw}");
        }
        for (raw, format) in [
            ("parse2files", Format::Camel),
            ("Parse2files", Format::Pascal),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&["parse2files"]), "{raw}");
            assert!(reading.admits(&["parse", "2files"]), "{raw}");
            // A lowercase letter continues the word.
            assert!(!reading.admits(&["parse", "2", "files"]), "{raw}");
            assert!(!reading.admits(&["parse2", "files"]), "{raw}");
        }
    }

    #[test]
    fn separated_formats_break_words_only_at_a_separator() {
        for (raw, format) in [
            ("parse2files", Format::Snake),
            ("PARSE2FILES", Format::ScreamingSnake),
            ("parse2files", Format::Kebab),
            ("PARSE2FILES", Format::ScreamingKebab),
            ("Parse2files", Format::Title),
            ("Parse2files", Format::Train),
            ("parse2files", Format::Dot),
            ("PARSE2FILES", Format::ScreamingDot),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&["parse2files"]), "{raw}");
            assert!(!reading.admits(&["parse", "2", "files"]), "{raw}");
            assert!(!reading.admits(&["parse", "2files"]), "{raw}");
        }
        let reading = candidates("parse_file_2", Format::Snake).unwrap();
        assert!(reading.admits(&["parse", "file", "2"]));
        assert!(!reading.admits(&["parse", "file2"]));
        let reading = candidates("Parse File2", Format::Title).unwrap();
        assert!(reading.admits(&["parse", "file2"]));
        assert!(!reading.admits(&["parse", "file", "2"]));
    }

    #[test]
    fn a_digit_agrees_across_formats_whichever_word_it_belongs_to() {
        let camel = candidates("parseFile2", Format::Camel).unwrap();
        for raw in ["parse_file2", "parse_file_2"] {
            let snake = candidates(raw, Format::Snake).unwrap();
            assert!(camel.narrow(&snake).is_some(), "{raw}");
        }
        let camel = candidates("parse2Files", Format::Camel).unwrap();
        let snake = candidates("parse2files", Format::Snake).unwrap();
        assert_eq!(camel.narrow(&snake), None);
    }

    #[test]
    fn title_and_train_capitals_inside_a_token_may_or_may_not_start_a_word() {
        for (raw, format) in [
            ("Parse2Files", Format::Title),
            ("Parse2Files", Format::Train),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&["parse2files"]), "{raw}");
            assert!(reading.admits(&["parse2", "files"]), "{raw}");
            // A digit still belongs to a word unless a separator is beside it.
            assert!(!reading.admits(&["parse", "2", "files"]), "{raw}");
        }
        let reading = candidates("Watch YouTube", Format::Title).unwrap();
        assert!(reading.admits(&["watch", "youtube"]));
        assert!(reading.admits(&["watch", "you", "tube"]));
        assert!(!reading.admits(&["watchyoutube"]));
    }

    #[test]
    fn different_characters_never_share_a_grouping() {
        let snake = candidates("parse_file", Format::Snake).unwrap();
        let other = candidates("parse_files", Format::Snake).unwrap();
        assert_eq!(snake.narrow(&other), None);
    }

    #[test]
    fn narrowing_keeps_only_the_groupings_both_admit() {
        let camel = candidates("parseXMLFile", Format::Camel).unwrap();
        let snake = candidates("parse_xml_file", Format::Snake).unwrap();
        let both = camel.narrow(&snake).unwrap();
        assert!(both.admits(&["parse", "xml", "file"]));
        assert!(!both.admits(&["parse", "x", "m", "l", "file"]));
        assert_eq!(
            both.narrow(&candidates("parse_x_m_l_file", Format::Snake).unwrap()),
            None
        );
        // `Either` on the other side narrows to this side's gap too.
        let both = snake.narrow(&camel).unwrap();
        assert!(both.admits(&["parse", "xml", "file"]));
        assert!(!both.admits(&["parse", "x", "m", "l", "file"]));
    }

    #[test]
    fn opaque_readings_agree_only_on_identical_text() {
        let value = Groupings::opaque("Some_Name");
        assert_eq!(
            value.narrow(&Groupings::opaque("Some_Name")),
            Some(value.clone())
        );
        assert_eq!(value.narrow(&Groupings::opaque("Some_Names")), None);
        assert!(value.admits(&["Some_Name"]));
        assert!(!value.admits(&["Some_", "Name"]));
    }

    #[test]
    fn a_long_ambiguous_value_is_read_without_listing_every_grouping() {
        // 2^199 groupings, if they were listed one by one.
        let raw = "A".repeat(200);
        let reading = candidates(&raw, Format::Pascal).unwrap();
        assert!(reading.narrow(&reading).is_some());
        assert_eq!(canonical_words(&raw, Some(Format::Pascal)).len(), 200);
    }

    #[test]
    fn leading_digit_word_makes_camel_and_pascal_coincide() {
        assert_eq!(
            candidates("25Values", Format::Camel),
            candidates("25Values", Format::Pascal)
        );
        assert!(
            candidates("25Values", Format::Camel)
                .unwrap()
                .admits(&["25", "values"])
        );
    }

    #[test]
    fn camel_rejects_a_leading_uppercase_word() {
        assert_eq!(candidates("XmlFile", Format::Camel), None);
    }

    #[test]
    fn pascal_rejects_a_leading_lowercase_word() {
        assert_eq!(candidates("xmlFile", Format::Pascal), None);
    }

    #[test]
    fn a_leading_digit_run_carries_on_into_lowercase_letters() {
        for format in [Format::Camel, Format::Pascal] {
            let reading = candidates("25fast", format).unwrap();
            assert!(reading.admits(&["25fast"]));
            assert!(!reading.admits(&["25", "fast"]));
        }
    }

    #[test]
    fn non_alphanumeric_characters_are_rejected_in_camel_and_pascal() {
        assert_eq!(candidates("foo bar", Format::Camel), None);
        assert_eq!(candidates("foo_bar", Format::Camel), None);
        assert_eq!(candidates("Foo_Bar", Format::Pascal), None);
    }

    #[test]
    fn empty_string_is_rejected_by_every_format() {
        for (_, format) in FILTER_NAMES {
            assert_eq!(candidates("", format), None);
        }
    }

    #[test]
    fn separated_formats_reject_the_wrong_separator() {
        assert_eq!(candidates("foo_bar", Format::Kebab), None);
        assert_eq!(candidates("foo-bar", Format::Snake), None);
        assert_eq!(candidates("Foo Bar", Format::Train), None);
        assert_eq!(candidates("Foo-Bar", Format::Title), None);
        assert_eq!(candidates("foo_bar", Format::Dot), None);
    }

    #[test]
    fn separated_formats_reject_the_wrong_case() {
        assert_eq!(candidates("Foo_bar", Format::Snake), None);
        assert_eq!(candidates("foo_bar", Format::ScreamingSnake), None);
        assert_eq!(candidates("foo-bar", Format::Train), None);
        assert_eq!(candidates("fOO-Bar", Format::Train), None);
        assert_eq!(candidates("foo.bar", Format::ScreamingDot), None);
        assert_eq!(candidates("FOO.BAR", Format::Dot), None);
    }

    #[test]
    fn separated_formats_reject_leading_trailing_or_doubled_separators() {
        assert_eq!(candidates("_foo", Format::Snake), None);
        assert_eq!(candidates("foo_", Format::Snake), None);
        assert_eq!(candidates("foo__bar", Format::Snake), None);
        assert_eq!(candidates(".foo", Format::Dot), None);
        assert_eq!(candidates("foo.", Format::Dot), None);
        assert_eq!(candidates("foo..bar", Format::Dot), None);
    }

    #[test]
    fn separated_formats_reject_other_characters_in_a_token() {
        assert_eq!(candidates("foo_b@r", Format::Snake), None);
        assert_eq!(candidates("FOO_B@R", Format::ScreamingSnake), None);
        assert_eq!(candidates("Foo B@r", Format::Title), None);
        assert_eq!(candidates("Foo Bär!", Format::Title), None);
    }

    #[test]
    fn title_and_train_accept_initialisms_and_capitals_inside_a_token() {
        for raw in [
            "XML",
            "B2B",
            "3D",
            "2files",
            "YouTube",
            "XMLFile",
            "Parse2Files",
        ] {
            assert!(candidates(raw, Format::Title).is_some(), "{raw}");
            assert!(candidates(raw, Format::Train).is_some(), "{raw}");
        }
        assert!(
            candidates("Parse-XMLFile", Format::Train)
                .unwrap()
                .admits(&["parse", "xml", "file"])
        );
    }

    #[test]
    fn title_and_train_reject_a_token_starting_lowercase() {
        assert_eq!(candidates("Watch youTube", Format::Title), None);
        assert_eq!(candidates("Watch-iPhone", Format::Train), None);
    }

    #[test]
    fn single_word_round_trips_through_every_format() {
        for (raw, format) in [
            ("value", Format::Camel),
            ("Value", Format::Pascal),
            ("value", Format::Snake),
            ("VALUE", Format::ScreamingSnake),
            ("value", Format::Kebab),
            ("VALUE", Format::ScreamingKebab),
            ("Value", Format::Title),
            ("Value", Format::Train),
            ("value", Format::Dot),
            ("VALUE", Format::ScreamingDot),
        ] {
            let reading = candidates(raw, format).unwrap();
            assert!(reading.admits(&["value"]), "{raw}");
            assert_eq!(canonical_words(raw, Some(format)), words(&["value"]));
        }
    }

    #[test]
    fn the_ascii_regex_fragment_agrees_with_the_full_one_on_ascii() {
        let samples = [
            "parseXMLFile",
            "ParseXMLFile",
            "parse2Files",
            "parseFile2",
            "Parse2files",
            "25Values",
            "25fast",
            "parse_x_m_l_file",
            "parse_2_files",
            "PARSE_XML_FILE",
            "PARSE2FILES",
            "parse-xml-file",
            "PARSE-FILE2",
            "parse.file2",
            "PARSE.XML.FILE",
            "Parse XML File",
            "Parse File2",
            "Parse-Xml-File",
            "Parse-XMLFile",
            "Parse XmL",
            "Foo_bar",
            "B2B",
            "3d",
            "iPhone",
            "Watch youTube",
            "foo__bar",
            "_foo",
            "foo-",
            "Foo  Bar",
            "Foo Bar ",
            "b@r",
            "",
        ];
        let anchored =
            |fragment: String| regex::Regex::new(&format!(r"\A(?:{fragment})\z")).unwrap();
        for (_, format) in FILTER_NAMES {
            let full = anchored(format.regex_fragment());
            let ascii = anchored(format.ascii_regex_fragment());
            let mut accepted = 0;
            for text in samples {
                assert_eq!(
                    ascii.is_match(text),
                    full.is_match(text),
                    "{format:?} disagrees with itself on {text:?}"
                );
                accepted += usize::from(ascii.is_match(text));
            }
            assert!(accepted > 0, "{format:?} accepted no sample");
            // Every ASCII character, on its own and after a letter.
            for c in (0..128u8).map(char::from) {
                for text in [c.to_string(), format!("a{c}"), format!("A{c}")] {
                    assert_eq!(
                        ascii.is_match(&text),
                        full.is_match(&text),
                        "{format:?} disagrees with itself on {text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_ascii_regex_fragment_accepts_no_other_character() {
        for (_, format) in FILTER_NAMES {
            let ascii =
                regex::Regex::new(&format!(r"\A(?:{})\z", format.ascii_regex_fragment())).unwrap();
            for text in ["é", "É", "语", "aé", "Aé", "a\u{301}"] {
                assert!(!ascii.is_match(text), "{format:?} accepted {text:?}");
            }
        }
    }

    #[test]
    fn regex_fragments_reject_a_shape_mismatch() {
        let camel = regex::Regex::new(&format!("^{}$", Format::Camel.regex_fragment())).unwrap();
        assert!(!camel.is_match("ParseXmlFile"));
        assert!(!camel.is_match("parse_xml_file"));
    }

    #[test]
    fn canonical_words_of_an_ambiguous_acronym_run_is_always_maximally_split() {
        let split = words(&["parse", "x", "m", "l", "file"]);
        assert_eq!(canonical_words("parseXMLFile", Some(Format::Camel)), split);
        assert_eq!(canonical_words("ParseXMLFile", Some(Format::Pascal)), split);
    }

    #[test]
    fn canonical_words_of_a_separated_format_is_its_one_unambiguous_grouping() {
        let expected = words(&["xml", "parser"]);
        assert_eq!(canonical_words("xml_parser", Some(Format::Snake)), expected);
        assert_eq!(
            canonical_words("XML_PARSER", Some(Format::ScreamingSnake)),
            expected
        );
        assert_eq!(canonical_words("xml-parser", Some(Format::Kebab)), expected);
        assert_eq!(canonical_words("Xml Parser", Some(Format::Title)), expected);
        assert_eq!(canonical_words("Xml-Parser", Some(Format::Train)), expected);
        assert_eq!(canonical_words("xml.parser", Some(Format::Dot)), expected);
        assert_eq!(
            canonical_words("XML.PARSER", Some(Format::ScreamingDot)),
            expected
        );
    }

    #[test]
    fn canonical_words_of_an_unformatted_value_is_one_opaque_word() {
        assert_eq!(
            canonical_words("anything_at_all", None),
            words(&["anything_at_all"])
        );
    }

    #[test]
    fn render_words_snake_sourced_segments_title_case_under_pascal_not_preserve_casing() {
        // Confirms the user-facing example: a `|snake` capture of
        // `xml_parser` (words ["xml", "parser"], already lowercase from
        // `candidates`' own normalization) renders as `XmlParser` under
        // `|pascal`, not `XMLParser` — rendering always title-cases each
        // word fresh rather than preserving any acronym-like casing.
        let words = canonical_words("xml_parser", Some(Format::Snake));
        assert_eq!(render_words(&words, Some(Format::Pascal)), "XmlParser");
    }

    #[test]
    fn render_words_round_trips_every_format_for_a_simple_two_word_value() {
        let words = words(&["parse", "file"]);
        assert_eq!(render_words(&words, Some(Format::Pascal)), "ParseFile");
        assert_eq!(render_words(&words, Some(Format::Camel)), "parseFile");
        assert_eq!(render_words(&words, Some(Format::Snake)), "parse_file");
        assert_eq!(
            render_words(&words, Some(Format::ScreamingSnake)),
            "PARSE_FILE"
        );
        assert_eq!(render_words(&words, Some(Format::Kebab)), "parse-file");
        assert_eq!(
            render_words(&words, Some(Format::ScreamingKebab)),
            "PARSE-FILE"
        );
        assert_eq!(render_words(&words, Some(Format::Title)), "Parse File");
        assert_eq!(render_words(&words, Some(Format::Train)), "Parse-File");
        assert_eq!(render_words(&words, Some(Format::Dot)), "parse.file");
        assert_eq!(
            render_words(&words, Some(Format::ScreamingDot)),
            "PARSE.FILE"
        );
    }

    #[test]
    fn render_words_maximally_split_acronym_round_trips_back_to_the_same_camel_pascal_spelling() {
        let words = canonical_words("parseXMLFile", Some(Format::Camel));
        assert_eq!(render_words(&words, Some(Format::Camel)), "parseXMLFile");
        assert_eq!(render_words(&words, Some(Format::Pascal)), "ParseXMLFile");
        assert_eq!(
            render_words(&words, Some(Format::Snake)),
            "parse_x_m_l_file"
        );
        assert_eq!(
            render_words(&words, Some(Format::Title)),
            "Parse X M L File"
        );
        assert_eq!(
            render_words(&words, Some(Format::Train)),
            "Parse-X-M-L-File"
        );
    }

    #[test]
    fn render_words_unformatted_returns_the_single_word_verbatim() {
        assert_eq!(
            render_words(&words(&["Anything_At_All"]), None),
            "Anything_At_All"
        );
    }

    #[test]
    fn canonical_words_split_camel_digits_but_keep_separated_segments_whole() {
        assert_eq!(
            canonical_words("parseFile2", Some(Format::Camel)),
            words(&["parse", "file", "2"])
        );
        assert_eq!(
            canonical_words("parse2Files", Some(Format::Camel)),
            words(&["parse", "2", "files"])
        );
        assert_eq!(
            canonical_words("parse2files", Some(Format::Camel)),
            words(&["parse2files"])
        );
        assert_eq!(
            canonical_words("XML25", Some(Format::Pascal)),
            words(&["x", "m", "l", "25"])
        );
        assert_eq!(
            canonical_words("parse_file2", Some(Format::Snake)),
            words(&["parse", "file2"])
        );
        assert_eq!(
            canonical_words("parse_2files", Some(Format::Snake)),
            words(&["parse", "2files"])
        );
        assert_eq!(
            canonical_words("Parse2files", Some(Format::Title)),
            words(&["parse2files"])
        );
        assert_eq!(
            canonical_words("Watch-YouTube", Some(Format::Train)),
            words(&["watch", "youtube"])
        );
    }

    #[test]
    fn render_words_capitalizes_only_the_start_of_a_word() {
        let words = words(&["parse2files", "x"]);
        assert_eq!(render_words(&words, Some(Format::Camel)), "parse2filesX");
        assert_eq!(render_words(&words, Some(Format::Pascal)), "Parse2filesX");
        assert_eq!(render_words(&words, Some(Format::Title)), "Parse2files X");
        assert_eq!(render_words(&words, Some(Format::Train)), "Parse2files-X");
        assert_eq!(render_words(&words, Some(Format::Snake)), "parse2files_x");
        assert_eq!(
            render_words(&words, Some(Format::ScreamingDot)),
            "PARSE2FILES.X"
        );
    }

    #[test]
    fn render_words_of_any_capture_is_accepted_back_by_every_format() {
        for (raw, source) in [
            ("parse2Files", Format::Camel),
            ("parse2files", Format::Camel),
            ("Parse XML File", Format::Title),
            ("Watch YouTube", Format::Title),
            ("parse_file2", Format::Snake),
            ("2Files", Format::Pascal),
            ("XML25", Format::Pascal),
        ] {
            let words = canonical_words(raw, Some(source));
            for (_, target) in FILTER_NAMES {
                let rendered = render_words(&words, Some(target));
                assert!(
                    candidates(&rendered, target).is_some(),
                    "{raw} rendered as {target:?} is {rendered:?}"
                );
            }
        }
    }

    #[test]
    fn a_word_starting_with_a_character_with_no_case_is_found_again_in_camel_and_pascal() {
        // Nothing marks the start of such a word in camel/pascal, so the
        // name a rule asks for has to be read back as the words it was
        // spelled from.
        for (raw, source) in [
            ("1-2-3", Format::Kebab),
            ("file_1_2", Format::Snake),
            ("12.3a", Format::Dot),
            ("日本-語", Format::Kebab),
            ("file_語", Format::Snake),
            ("2 語", Format::Title),
        ] {
            let words = canonical_words(raw, Some(source));
            for target in [Format::Camel, Format::Pascal] {
                let rendered = render_words(&words, Some(target));
                assert!(
                    agree(raw, source, &rendered, target),
                    "{raw} rendered as {target:?} is {rendered:?}"
                );
            }
        }
        let reading = candidates("123", Format::Pascal).unwrap();
        assert!(reading.admits(&["1", "2", "3"]));
        assert!(reading.admits(&["12", "3"]));
        assert!(reading.admits(&["123"]));
        assert!(!agree("123", Format::Pascal, "1-3-2", Format::Kebab));
    }

    #[test]
    fn canonical_words_never_split_a_run_of_characters_with_no_case() {
        for (raw, expected) in [
            ("123", vec!["123"]),
            ("parseFile25", vec!["parse", "file", "25"]),
            ("parse25files", vec!["parse25files"]),
            ("日本語", vec!["日本語"]),
            ("File日本語", vec!["file日本語"]),
            ("file2語", vec!["file2語"]),
            ("File語2", vec!["file語", "2"]),
        ] {
            for format in [Format::Camel, Format::Pascal] {
                if candidates(raw, format).is_some() {
                    assert_eq!(
                        canonical_words(raw, Some(format)),
                        words(&expected),
                        "{raw}"
                    );
                }
            }
        }
    }

    /// `raw` (captured under `format`) and `other` (under `other_format`)
    /// are spellings of one identifier.
    fn agree(raw: &str, format: Format, other: &str, other_format: Format) -> bool {
        let reading = candidates(raw, format).unwrap_or_else(|| panic!("{raw} isn't {format:?}"));
        let other_reading = candidates(other, other_format)
            .unwrap_or_else(|| panic!("{other} isn't {other_format:?}"));
        reading.narrow(&other_reading).is_some()
    }

    /// One two-word value spelled in each of the ten formats.
    fn spelled_ten_ways(spellings: [&str; 10]) -> [(&str, Format); 10] {
        let formats = [
            Format::Camel,
            Format::Pascal,
            Format::Snake,
            Format::ScreamingSnake,
            Format::Kebab,
            Format::ScreamingKebab,
            Format::Title,
            Format::Train,
            Format::Dot,
            Format::ScreamingDot,
        ];
        std::array::from_fn(|index| (spellings[index], formats[index]))
    }

    #[test]
    fn accented_letters_canonicalize_consistently_across_every_format() {
        let expected = words(&["crème", "brûlée"]);
        let spellings = spelled_ten_ways([
            "crèmeBrûlée",
            "CrèmeBrûlée",
            "crème_brûlée",
            "CRÈME_BRÛLÉE",
            "crème-brûlée",
            "CRÈME-BRÛLÉE",
            "Crème Brûlée",
            "Crème-Brûlée",
            "crème.brûlée",
            "CRÈME.BRÛLÉE",
        ]);
        for (raw, format) in spellings {
            assert_eq!(canonical_words(raw, Some(format)), expected, "{raw}");
            for (other, other_format) in spellings {
                assert!(agree(raw, format, other, other_format), "{raw} vs {other}");
                assert_eq!(render_words(&expected, Some(other_format)), other);
            }
        }
    }

    #[test]
    fn a_single_accented_word_fits_the_formats_of_its_case() {
        for format in [Format::Camel, Format::Snake, Format::Kebab, Format::Dot] {
            assert_eq!(
                canonical_words("über", Some(format)),
                words(&["über"]),
                "{format:?}"
            );
            assert!(candidates("Über", format).is_none(), "{format:?}");
            assert!(candidates("ÜBER", format).is_none(), "{format:?}");
        }
        for format in [Format::Pascal, Format::Title, Format::Train] {
            assert_eq!(
                canonical_words("Über", Some(format)),
                words(&["über"]),
                "{format:?}"
            );
            assert!(candidates("über", format).is_none(), "{format:?}");
        }
        for format in [
            Format::ScreamingSnake,
            Format::ScreamingKebab,
            Format::ScreamingDot,
        ] {
            assert_eq!(
                canonical_words("ÜBER", Some(format)),
                words(&["über"]),
                "{format:?}"
            );
            assert!(candidates("über", format).is_none(), "{format:?}");
            assert!(candidates("Über", format).is_none(), "{format:?}");
        }
    }

    #[test]
    fn an_accented_capital_starts_a_word_in_camel_and_pascal() {
        let expected = words(&["école", "élémentaire"]);
        assert_eq!(
            canonical_words("écoleÉlémentaire", Some(Format::Camel)),
            expected
        );
        assert_eq!(
            canonical_words("ÉcoleÉlémentaire", Some(Format::Pascal)),
            expected
        );
        assert!(candidates("ÉcoleÉlémentaire", Format::Camel).is_none());
        assert!(candidates("écoleÉlémentaire", Format::Pascal).is_none());
        assert!(agree(
            "écoleÉlémentaire",
            Format::Camel,
            "école_élémentaire",
            Format::Snake
        ));
        assert!(!agree(
            "écoleélémentaire",
            Format::Camel,
            "école_élémentaire",
            Format::Snake
        ));
    }

    #[test]
    fn a_run_of_accented_capitals_may_or_may_not_be_one_word() {
        let reading = candidates("lesÉÉUFrance", Format::Camel).unwrap();
        assert!(reading.admits(&["les", "ééu", "france"]));
        assert!(reading.admits(&["les", "é", "é", "u", "france"]));
        assert!(!reading.admits(&["lesééu", "france"]));
        assert_eq!(
            canonical_words("lesÉÉUFrance", Some(Format::Camel)),
            words(&["les", "é", "é", "u", "france"])
        );
        let title = candidates("Les ÉÉU", Format::Title).unwrap();
        assert!(title.admits(&["les", "ééu"]));
        assert!(title.admits(&["les", "é", "é", "u"]));
    }

    #[test]
    fn an_accent_is_part_of_the_letter_not_a_variant_of_it() {
        assert!(!agree("uber", Format::Snake, "über", Format::Kebab));
        assert!(!agree(
            "cafe",
            Format::Snake,
            "CAFÉ",
            Format::ScreamingSnake
        ));
        assert!(!agree("résumé", Format::Snake, "Resume", Format::Pascal));
        assert!(!agree("año", Format::Camel, "Ano", Format::Pascal));
    }

    #[test]
    fn other_alphabets_with_capitals_work_like_latin() {
        // Greek, Cyrillic and Armenian.
        for (camel, pascal, snake, screaming, expected) in [
            (
                "καλημέραΚόσμε",
                "ΚαλημέραΚόσμε",
                "καλημέρα_κόσμε",
                "ΚΑΛΗΜΈΡΑ_ΚΌΣΜΕ",
                ["καλημέρα", "κόσμε"],
            ),
            (
                "приветМир",
                "ПриветМир",
                "привет_мир",
                "ПРИВЕТ_МИР",
                ["привет", "мир"],
            ),
            (
                "բարևԱշխարհ",
                "ԲարևԱշխարհ",
                "բարև_աշխարհ",
                "ԲԱՐԵՎ_ԱՇԽԱՐՀ",
                ["բարև", "աշխարհ"],
            ),
        ] {
            let expected = words(&expected);
            assert_eq!(canonical_words(camel, Some(Format::Camel)), expected);
            assert_eq!(canonical_words(pascal, Some(Format::Pascal)), expected);
            assert_eq!(canonical_words(snake, Some(Format::Snake)), expected);
            assert!(agree(camel, Format::Camel, pascal, Format::Pascal));
            assert!(agree(camel, Format::Camel, snake, Format::Snake));
            assert!(candidates(snake, Format::ScreamingSnake).is_none());
            assert!(candidates(screaming, Format::Snake).is_none());
            assert_eq!(render_words(&expected, Some(Format::Camel)), camel);
            assert_eq!(render_words(&expected, Some(Format::Pascal)), pascal);
        }
        // Armenian's `և` has no single capital (it's written `ԵՎ`), so the
        // two-letter spelling is a different name; the other two agree.
        assert!(agree(
            "привет_мир",
            Format::Snake,
            "ПРИВЕТ_МИР",
            Format::ScreamingSnake
        ));
        assert!(agree(
            "καλημέρα_κόσμε",
            Format::Snake,
            "ΚΑΛΗΜΈΡΑ_ΚΌΣΜΕ",
            Format::ScreamingSnake
        ));
        assert!(!agree(
            "բարև_աշխարհ",
            Format::Snake,
            "ԲԱՐԵՎ_ԱՇԽԱՐՀ",
            Format::ScreamingSnake
        ));
    }

    #[test]
    fn a_script_without_capitals_fits_every_format() {
        // Japanese, Chinese, Korean, Hebrew, Arabic, Hindi and Thai: none
        // has upper or lower case, so a word in one is neither.
        for word in ["日本語", "文件", "한국어", "שלום", "مرحبا", "हिन्दी", "สวัสดี"]
        {
            for (_, format) in FILTER_NAMES {
                assert_eq!(
                    canonical_words(word, Some(format)),
                    words(&[word]),
                    "{word} as {format:?}"
                );
            }
        }
    }

    #[test]
    fn an_emoji_fits_every_format() {
        // One character; a thumbs-up and a skin tone; a family, which is
        // three people with joiners between them; a heart, which is a
        // symbol and the mark that draws it as an emoji; a flag, which is
        // two regional letters; and a flag spelled out in tag characters.
        for word in [
            "😀",
            "👍🏽",
            "👨\u{200d}👩\u{200d}👧",
            "❤\u{fe0f}",
            "🇯🇵",
            "🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
        ] {
            for (_, format) in FILTER_NAMES {
                assert!(candidates(word, format).is_some(), "{word} as {format:?}");
                assert_eq!(
                    canonical_words(word, Some(format)),
                    words(&[word]),
                    "{word} as {format:?}"
                );
            }
        }
    }

    #[test]
    fn an_emoji_breaks_words_only_at_a_separator_or_a_capital() {
        assert_eq!(
            canonical_words("😀-😀", Some(Format::Kebab)),
            words(&["😀", "😀"])
        );
        assert!(agree("😀-😀", Format::Kebab, "😀_😀", Format::Snake));
        assert!(agree("😀-😀", Format::Kebab, "😀😀", Format::Pascal));
        assert!(!agree("😀-😀", Format::Kebab, "😀_😃", Format::Snake));
        assert_eq!(
            canonical_words("party🎉Time", Some(Format::Camel)),
            words(&["party🎉", "time"])
        );
        assert!(agree(
            "party🎉Time",
            Format::Camel,
            "PARTY🎉_TIME",
            Format::ScreamingSnake
        ));
    }

    #[test]
    fn punctuation_and_symbols_that_are_not_emoji_fit_no_format() {
        // `#` and `*` are emoji to Unicode, for their keycaps, and `→`
        // and `€` are symbols but not emoji.
        for raw in ["a#b", "a*b", "a→b", "a€b", "a!b"] {
            for (_, format) in FILTER_NAMES {
                assert!(candidates(raw, format).is_none(), "{raw} as {format:?}");
            }
        }
    }

    #[test]
    fn a_script_without_capitals_breaks_words_only_at_a_separator_or_a_capital() {
        let expected = words(&["日本語", "ファイル"]);
        for (raw, format) in [
            ("日本語_ファイル", Format::Snake),
            ("日本語_ファイル", Format::ScreamingSnake),
            ("日本語-ファイル", Format::Kebab),
            ("日本語-ファイル", Format::Train),
            ("日本語 ファイル", Format::Title),
            ("日本語.ファイル", Format::Dot),
        ] {
            assert_eq!(canonical_words(raw, Some(format)), expected, "{raw}");
        }
        assert_eq!(
            canonical_words("日本語ファイル", Some(Format::Camel)),
            words(&["日本語ファイル"])
        );
        assert_eq!(
            canonical_words("日本語File", Some(Format::Camel)),
            words(&["日本語", "file"])
        );
        assert_eq!(
            canonical_words("File日本語", Some(Format::Pascal)),
            words(&["file日本語"])
        );
        assert!(agree(
            "日本語File",
            Format::Pascal,
            "日本語_file",
            Format::Snake
        ));
    }

    #[test]
    fn sharp_s_is_lowercase_and_its_capital_is_the_single_letter() {
        assert_eq!(
            canonical_words("straße", Some(Format::Snake)),
            words(&["straße"])
        );
        assert_eq!(
            canonical_words("STRAẞE", Some(Format::ScreamingSnake)),
            words(&["straße"])
        );
        assert!(agree(
            "straße",
            Format::Snake,
            "STRAẞE",
            Format::ScreamingSnake
        ));
        assert!(agree(
            "große_straße",
            Format::Snake,
            "GroßeStraße",
            Format::Pascal
        ));
        // `ß` is a lowercase letter, so it can't be in a capitals-only name.
        assert!(candidates("STRAßE", Format::ScreamingSnake).is_none());
        // The two-letter capital spelling is a different name: undoing it
        // would be a guess (`MASSE` is `masse`, or `maße`).
        assert!(!agree(
            "straße",
            Format::Snake,
            "STRASSE",
            Format::ScreamingSnake
        ));
    }

    #[test]
    fn rendering_never_changes_how_many_letters_a_word_has() {
        let straße = words(&["straße"]);
        assert_eq!(
            render_words(&straße, Some(Format::ScreamingSnake)),
            "STRAẞE"
        );
        assert_eq!(render_words(&straße, Some(Format::Pascal)), "Straße");
        assert_eq!(render_words(&words(&["ßeta"]), Some(Format::Title)), "ẞeta");
        // `ŉ` has no single capital either (it would be `ʼN`), and is
        // rare enough to be left as it is.
        assert_eq!(
            render_words(&words(&["ŉ"]), Some(Format::ScreamingDot)),
            "ŉ"
        );
    }

    #[test]
    fn dotted_and_dotless_i_are_all_the_letter_i() {
        // Turkish has two: `i`/`İ` and `ı`/`I`. Telling which a name means
        // needs its language, which a file name doesn't give.
        assert_eq!(
            canonical_words("İSTANBUL", Some(Format::ScreamingSnake)),
            words(&["istanbul"])
        );
        assert_eq!(
            canonical_words("İstanbul", Some(Format::Pascal)),
            words(&["istanbul"])
        );
        assert!(candidates("İstanbul", Format::Camel).is_none());
        assert!(candidates("ıspanak", Format::Pascal).is_none());
        for (lower, upper) in [
            ("istanbul", "ISTANBUL"),
            ("istanbul", "İSTANBUL"),
            ("diyarbakır", "DIYARBAKIR"),
            ("diyarbakir", "DIYARBAKIR"),
            ("diyarbakır", "DİYARBAKİR"),
        ] {
            assert!(
                agree(lower, Format::Snake, upper, Format::ScreamingSnake),
                "{lower} vs {upper}"
            );
        }
        assert!(agree("ılık_su", Format::Snake, "IlıkSu", Format::Pascal));
        assert_eq!(
            render_words(&words(&["diyarbakır"]), Some(Format::ScreamingKebab)),
            "DIYARBAKIR"
        );
        assert_eq!(
            render_words(&words(&["istanbul"]), Some(Format::Pascal)),
            "Istanbul"
        );
        // Still a lowercase letter of its own when nothing recases it.
        assert_eq!(
            render_words(&words(&["ılık", "su"]), Some(Format::Kebab)),
            "ılık-su"
        );
    }

    #[test]
    fn letters_with_one_capital_and_two_lowercase_forms_agree() {
        // Greek's word-final `ς` and `σ` are both `Σ` in capitals.
        assert!(agree("οδος", Format::Snake, "ΟΔΟΣ", Format::ScreamingSnake));
        assert!(agree("οδός_μου", Format::Snake, "ΟδόςΜου", Format::Pascal));
        assert_eq!(
            canonical_words("οδός_μου", Some(Format::Snake)),
            words(&["οδός", "μου"])
        );
        assert_eq!(
            render_words(&words(&["οδός", "μου"]), Some(Format::Kebab)),
            "οδός-μου"
        );
        // The long s, `ſ`, is a second lowercase form of `S`.
        assert!(agree("ſo", Format::Snake, "SO", Format::ScreamingSnake));
    }

    #[test]
    fn an_unformatted_value_is_never_case_folded() {
        let lower = Groupings::opaque("οδος");
        assert!(lower.narrow(&Groupings::opaque("οδοσ")).is_none());
        assert!(lower.narrow(&Groupings::opaque("ΟΔΟΣ")).is_none());
        assert!(
            Groupings::opaque("ı")
                .narrow(&Groupings::opaque("i"))
                .is_none()
        );
        assert!(lower.narrow(&Groupings::opaque("οδος")).is_some());
    }

    #[test]
    fn a_combining_accent_stays_with_its_letter() {
        // `x̄` has no single-character form, so it's always `x` and a
        // combining macron; flint composes the pairs that do (see `run`).
        for (raw, format, expected) in [
            ("x\u{304}_bar", Format::Snake, ["x\u{304}", "bar"]),
            ("X\u{304}_BAR", Format::ScreamingSnake, ["x\u{304}", "bar"]),
            ("x\u{304}Bar", Format::Camel, ["x\u{304}", "bar"]),
            ("X\u{304}Bar", Format::Pascal, ["x\u{304}", "bar"]),
            ("barX\u{304}", Format::Camel, ["bar", "x\u{304}"]),
            ("cafe\u{301}-bar", Format::Kebab, ["cafe\u{301}", "bar"]),
        ] {
            assert_eq!(
                canonical_words(raw, Some(format)),
                words(&expected),
                "{raw}"
            );
        }
        assert!(agree(
            "x\u{304}Bar",
            Format::Camel,
            "X\u{304}_BAR",
            Format::ScreamingSnake
        ));
        assert_eq!(
            render_words(&words(&["x\u{304}", "bar"]), Some(Format::Pascal)),
            "X\u{304}Bar"
        );
    }

    #[test]
    fn digits_of_any_script_are_digits() {
        // Arabic-Indic `٣` and Devanagari `३`.
        for digit in ['٣', '३'] {
            let raw = format!("file{digit}");
            let reading = candidates(&raw, Format::Camel).unwrap();
            let (file, joined) = (String::from("file"), raw.clone());
            assert!(reading.admits(&[&file, &digit.to_string()]), "{raw}");
            assert!(reading.admits(&[&joined]), "{raw}");
            assert_eq!(
                canonical_words(&raw, Some(Format::Camel)),
                vec![file, digit.to_string()]
            );
            assert_eq!(
                canonical_words(&raw, Some(Format::Snake)),
                vec![joined],
                "{raw}"
            );
            for (_, format) in FILTER_NAMES {
                assert!(
                    candidates(&digit.to_string(), format).is_some(),
                    "{digit} as {format:?}"
                );
            }
        }
    }

    #[test]
    fn accented_digits_and_letters_mix_like_ascii_ones() {
        assert_eq!(
            canonical_words("größe2Über", Some(Format::Camel)),
            words(&["größe", "2", "über"])
        );
        assert_eq!(
            canonical_words("größe2über", Some(Format::Camel)),
            words(&["größe2über"])
        );
        assert_eq!(
            canonical_words("2Écoles", Some(Format::Pascal)),
            words(&["2", "écoles"])
        );
        assert!(agree("größe2", Format::Camel, "größe-2", Format::Kebab));
        assert!(agree(
            "größe2",
            Format::Camel,
            "GRÖẞE2",
            Format::ScreamingDot
        ));
    }

    #[test]
    fn symbols_and_punctuation_are_rejected_whatever_the_script() {
        for raw in [
            "naïve!",
            "café?",
            "über@",
            "a→b",
            "€uro",
            "日本語。",
            "a\u{a0}b",
            "a\u{200b}b",
            "¿qué",
        ] {
            for (_, format) in FILTER_NAMES {
                assert!(candidates(raw, format).is_none(), "{raw:?} as {format:?}");
            }
        }
    }

    #[test]
    fn only_the_ascii_separator_separates() {
        // Lookalikes of `_`, `-`, `.` and the space aren't separators (nor
        // letters), so a name using one doesn't fit.
        assert!(candidates("crème＿brûlée", Format::Snake).is_none());
        assert!(candidates("crème‐brûlée", Format::Kebab).is_none());
        assert!(candidates("crème–brûlée", Format::Kebab).is_none());
        assert!(candidates("crème。brûlée", Format::Dot).is_none());
        assert!(candidates("Crème\u{a0}Brûlée", Format::Title).is_none());
        assert!(candidates("Crème\u{3000}Brûlée", Format::Title).is_none());
    }

    #[test]
    fn accented_names_reject_the_wrong_separator_or_a_doubled_one() {
        assert!(candidates("crème-brûlée", Format::Snake).is_none());
        assert!(candidates("crème_brûlée", Format::Kebab).is_none());
        assert!(candidates("crème__brûlée", Format::Snake).is_none());
        assert!(candidates("_crème", Format::Snake).is_none());
        assert!(candidates("crème.", Format::Dot).is_none());
        assert!(candidates("Crème brûlée", Format::Title).is_none());
        assert!(candidates("Crème-brûlée", Format::Train).is_none());
        assert!(candidates("CRÈME_brûlée", Format::ScreamingSnake).is_none());
        assert!(candidates("crème_Brûlée", Format::Snake).is_none());
    }

    #[test]
    fn rendering_an_accented_capture_is_accepted_back_as_the_same_identifier() {
        for (raw, source) in [
            ("crèmeBrûlée", Format::Camel),
            ("ÉcoleÉlémentaire2", Format::Pascal),
            ("große_straße", Format::Snake),
            ("GROẞE_STRAẞE", Format::ScreamingSnake),
            ("İstanbul Boğazı", Format::Title),
            ("ılık-su", Format::Kebab),
            ("ΟΔΌΣ.ΜΟΥ", Format::ScreamingDot),
            ("οδός-μου", Format::Kebab),
            ("приветМир", Format::Camel),
            ("日本語File", Format::Pascal),
            ("x\u{304}Bar", Format::Camel),
            ("ǅungla", Format::Pascal),
        ] {
            let words = canonical_words(raw, Some(source));
            for (_, target) in FILTER_NAMES {
                let rendered = render_words(&words, Some(target));
                assert!(
                    agree(raw, source, &rendered, target),
                    "{raw} rendered as {target:?} is {rendered:?}"
                );
            }
        }
    }

    #[test]
    fn a_capture_no_format_would_make_is_still_read_without_panicking() {
        // Only reachable if the regex and the standard library disagree on
        // a character, as two versions of Unicode can.
        for (_, format) in FILTER_NAMES {
            for raw in ["", "_", "__a", "a--b ", "!", "..", " A"] {
                let reading = groupings(raw, format);
                assert_eq!(reading.gaps.len(), reading.chars.len().saturating_sub(1));
                let words = canonical_words(raw, Some(format));
                assert_eq!(words.concat().chars().count(), reading.chars.len());
            }
        }
    }
}
