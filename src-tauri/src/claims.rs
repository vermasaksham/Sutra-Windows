//! Numeric claims in prose, and when two of them disagree.
//!
//! # What this is, and firmly is not
//!
//! Detecting that two passages of prose contradict each other is a research
//! problem, not a feature. Detecting that two numbers written as the same
//! quantity in the same unit differ by a factor is arithmetic. This ships only
//! the arithmetic, and the panel says so: **two numeric claims differ**, never
//! "these notes contradict". Which of them is right, or whether they are even
//! about the same measurement, is the reader's to decide and is not knowable
//! from the text.
//!
//! # What counts as a claim
//!
//! `κ = 0.037 W m⁻¹ K⁻¹` — a label, a separator, a number, a unit. The
//! separator is required. Prose is full of numbers ("ramped to 800 K", "the
//! third run", "10 K/min"), and a number nobody wrote as an assignment is a
//! number nobody was claiming; picking those up would flag a ramp's start
//! against its end and teach the reader to ignore the panel by the second
//! note.
//!
//! Requiring the label is what makes the comparison mean anything. Two
//! temperatures in kelvin are not in disagreement for being different
//! temperatures; two values of `κ` in the same unit are worth a second look.
//!
//! # What it will miss
//!
//! Anything not written as an assignment; anything whose unit is spelled in a
//! way [`canonical_unit`] cannot line up with the other spelling; and any
//! disagreement that is not arithmetic. Those are false negatives, which cost
//! nothing — the vault works as it did. The thing being avoided is the false
//! positive, which costs attention every time and buys nothing.

use serde::Serialize;

/// One numeric claim, as written and as compared.
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    /// The label as it was typed: `κ`, `Cp`, `thermal conductivity`.
    pub label: String,
    /// Lowercased and stripped, for matching one claim to another.
    pub key: String,
    pub value: f64,
    /// The unit as typed, for showing back.
    pub unit: String,
    /// Reordered and expanded, for comparing. Empty when there was no unit.
    pub unit_key: String,
    /// The whole claim as it appears in the note, for quoting in the panel.
    pub text: String,
}

/// Two claims of the same thing, in two notes, that differ.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disagreement {
    /// What both claims are about, as this note wrote it.
    pub label: String,
    /// The claim in the open note, as written.
    pub here: String,
    /// The other note, and its claim.
    pub id: String,
    pub title: String,
    pub there: String,
    /// How many times apart the two values are, larger over smaller.
    pub factor: f64,
}

/// How far apart two values must be before it is worth saying anything.
///
/// Measurements of the same quantity disagree by a few percent as a matter of
/// course; that is precision, not disagreement, and flagging it would fill the
/// panel with noise. A factor of two is past anything precision explains, and
/// in a materials vault is the size of difference that turns out to be a unit
/// error, a typo, or a genuinely different sample.
pub const FACTOR: f64 = 2.0;

/// The longest a label may be.
///
/// A quantity is named in a word or two. A long run of text before an `=` is a
/// sentence that happens to end in one, and its "label" would match nothing.
const LABEL_LIMIT: usize = 32;

/// Do these two claims disagree?
///
/// Same quantity, same unit, values more than [`FACTOR`] apart. A claim with
/// no unit is only ever compared with another that has none — a bare `κ = 0.3`
/// could be in any unit, and guessing which would be inventing the fact the
/// comparison rests on.
pub fn disagree(a: &Claim, b: &Claim) -> bool {
    if a.key != b.key || a.unit_key != b.unit_key {
        return false;
    }
    factor(a.value, b.value).is_some_and(|f| f >= FACTOR)
}

/// How many times apart two values are, larger over smaller.
///
/// `None` when either is zero or they have opposite signs: "twice as much as
/// nothing" is not a ratio, and a sign change is a different kind of
/// disagreement than this can speak to.
pub fn factor(a: f64, b: f64) -> Option<f64> {
    if a == 0.0 || b == 0.0 || a.signum() != b.signum() {
        return None;
    }
    let (a, b) = (a.abs(), b.abs());
    Some(if a > b { a / b } else { b / a })
}

/// Every numeric claim in a note's body.
pub fn claims(body: &str) -> Vec<Claim> {
    let mut out = Vec::new();
    for line in body.lines() {
        // A line at a time, so a label can never be read across a line break
        // and pick up the end of the sentence above it.
        out.extend(claims_in_line(line));
    }
    out
}

fn claims_in_line(line: &str) -> Vec<Claim> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let separator = match chars[i] {
            '=' | ':' | '≈' | '~' => 1,
            // " of " — written out, and the only prose form accepted. Anything
            // looser starts matching sentences rather than claims.
            'o' if matches(&chars, i, "of ") && i > 0 && chars[i - 1] == ' ' => 3,
            _ => {
                i += 1;
                continue;
            }
        };
        let mut end = i + separator;
        while end < chars.len() && matches!(chars[end], '=' | ':') {
            end += 1;
        }
        // `==`, `:=` and `::` are code or syntax, not somebody claiming a
        // measurement — and a research vault has code in it.
        let doubled = separator == 1 && end > i + 1;

        if !doubled {
            if let Some(claim) = read_claim(&chars, i, end) {
                out.push(claim);
            }
        }
        i = end.max(i + 1);
    }
    out
}

fn matches(chars: &[char], at: usize, word: &str) -> bool {
    word.chars()
        .enumerate()
        .all(|(n, c)| chars.get(at + n) == Some(&c))
}

/// Read a claim whose separator occupies `start..end`.
fn read_claim(chars: &[char], start: usize, end: usize) -> Option<Claim> {
    let label = read_label(chars, start)?;
    let (value, unit, after) = read_value(chars, end)?;

    let text: String = chars[label.1..after].iter().collect();
    Some(Claim {
        key: normalise_label(&label.0)?,
        label: label.0,
        value,
        unit_key: canonical_unit(&unit),
        unit,
        text: text.trim().to_string(),
    })
}

/// The label before the separator, and where it starts.
fn read_label(chars: &[char], separator: usize) -> Option<(String, usize)> {
    let mut end = separator;
    while end > 0 && chars[end - 1] == ' ' {
        end -= 1;
    }
    let mut start = end;
    let mut words = 0;
    while start > 0 && end - start <= LABEL_LIMIT {
        let c = chars[start - 1];
        if c == ' ' {
            // A label may be a short phrase — "thermal conductivity of …" —
            // but not a sentence. Two spaces back is as far as it reaches.
            if words >= 2 {
                break;
            }
            words += 1;
            start -= 1;
            continue;
        }
        // Anything that is not part of a name ends it: a bullet, a bracket,
        // the end of the previous sentence.
        if !(c.is_alphanumeric() || matches!(c, '_' | '-' | '^' | '\'' | '′')) {
            break;
        }
        start -= 1;
    }
    let label: String = chars[start..end].iter().collect();
    let label = label.trim().to_string();
    (!label.is_empty() && label.chars().any(char::is_alphabetic)).then_some((label, start))
}

/// The number and unit after the separator, and where they end.
fn read_value(chars: &[char], from: usize) -> Option<(f64, String, usize)> {
    let mut i = from;
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    let start = i;
    if i < chars.len() && matches!(chars[i], '+' | '-' | '−') {
        i += 1;
    }
    let digits = i;
    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
        i += 1;
    }
    if i == digits {
        return None;
    }
    // `1e-3` and `1E-3`, but not the `e` of a word running on.
    if i < chars.len() && matches!(chars[i], 'e' | 'E') {
        let mut j = i + 1;
        if j < chars.len() && matches!(chars[j], '+' | '-' | '−') {
            j += 1;
        }
        let exponent = j;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j > exponent {
            i = j;
        }
    }

    let raw: String = chars[start..i].iter().collect();
    let value: f64 = raw.replace('−', "-").parse().ok()?;

    // A range is not a claim. "300–800 K" is a ramp, and comparing its ends
    // with another note's would be comparing two things nobody asserted.
    let mut after = i;
    while after < chars.len() && chars[after] == ' ' {
        after += 1;
    }
    if after < chars.len() && matches!(chars[after], '-' | '–' | '—') {
        return None;
    }
    if matches(chars, after, "to ") {
        return None;
    }

    // The unit: one run of unit characters, then as many further runs as
    // still look like unit factors.
    //
    // The last part is the fiddly one. `0.037 W m⁻¹ K⁻¹ at 300 K` has a unit
    // of three space-separated factors followed by a word that is not one, and
    // reading "at" into the unit would make the claim compare with nothing.
    let unit_start = after;
    let mut end = run_end(chars, after);
    while end < chars.len() && chars[end] == ' ' {
        let next = run_end(chars, end + 1);
        let run: String = chars[end + 1..next].iter().collect();
        if next == end + 1 || !is_unit_factor(&run) {
            break;
        }
        end = next;
    }
    let unit: String = chars[unit_start..end].iter().collect();
    let unit = unit.trim().to_string();
    // With no unit the claim ends at the number, so the quoted text does not
    // trail off into the rest of the sentence.
    let after = if unit.is_empty() { i } else { end };
    Some((value, unit, after))
}

fn is_superscript(c: char) -> bool {
    matches!(c, '⁰'..='⁹' | '⁻' | '⁺' | '¹' | '²' | '³')
}

/// Where the run of unit characters starting at `from` ends.
fn run_end(chars: &[char], from: usize) -> usize {
    let mut end = from;
    while end < chars.len() {
        let c = chars[end];
        if c.is_alphanumeric()
            || matches!(
                c,
                '/' | '·' | '⋅' | '^' | '-' | '−' | '%' | '°' | 'µ' | '(' | ')'
            )
            || is_superscript(c)
        {
            end += 1;
        } else {
            break;
        }
    }
    end
}

/// Whether a run after a space belongs to the unit rather than to the sentence.
///
/// Either it carries an exponent or a solidus — `K⁻¹`, `m^-1`, `J/mol` are
/// nobody's English — or it is made entirely of known symbols. "at", "and",
/// "in" and every other word that follows a unit satisfy neither.
fn is_unit_factor(run: &str) -> bool {
    if run.is_empty() {
        return false;
    }
    if run.contains(['/', '^', '%', '°'])
        || run.chars().any(is_superscript)
        || run.chars().any(|c| c.is_ascii_digit())
    {
        return true;
    }
    let symbols = split_symbols(run);
    symbols.iter().all(|s| SYMBOLS.contains(&s.as_str()))
}

/// A label reduced to what it names.
fn normalise_label(label: &str) -> Option<String> {
    let key: String = label
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // A label of nothing but digits names nothing.
    (!key.is_empty() && key.chars().any(char::is_alphabetic)).then_some(key)
}

/// The SI symbols an unseparated run may be split into.
///
/// Longest first, so `mol` is read before `m`. Only used when the whole run is
/// not itself a symbol, so `Pa`, `Hz` and `mol` survive intact and `mK` in
/// `W/mK` becomes metre-kelvin.
///
/// `mK` really can mean millikelvin, and in a cryogenics vault it will. The
/// cost of reading it as metre-kelvin is that a millikelvin claim compares
/// only with other millikelvin claims — a comparison not made, which is the
/// direction this whole module errs in.
const SYMBOLS: &[&str] = &[
    "mol", "Ohm", "rad", "Pa", "Hz", "eV", "cd", "sr", "kg", "Wb", "lm", "lx", "Bq", "Gy", "Sv",
    "kat", "K", "A", "s", "m", "g", "N", "J", "W", "C", "V", "F", "S", "T", "H", "L", "Ω", "°",
];

/// A unit rewritten so two spellings of it come out the same.
///
/// Superscripts become `^n`, everything after a solidus is inverted, factors
/// are split and sorted. `W m⁻¹ K⁻¹` and `W/mK` and `W/(m·K)` all reach
/// `K^-1 W^1 m^-1`.
///
/// Case is kept: `m` and `M`, `s` and `S` are different units, and lowercasing
/// would silently equate them.
pub fn canonical_unit(unit: &str) -> String {
    let unit = unit.trim();
    if unit.is_empty() {
        return String::new();
    }
    let expanded = expand_superscripts(unit).replace(['(', ')', '−'], "");

    let mut factors: Vec<(String, i32)> = Vec::new();
    // Everything after the first solidus is in the denominator. `a/b/c` is
    // ambiguous in ordinary writing and is read here the way most people write
    // it: everything below the line.
    for (part, sign) in expanded
        .split('/')
        .enumerate()
        .map(|(n, part)| (part, if n == 0 { 1 } else { -1 }))
    {
        for run in part.split([' ', '·', '⋅', '*']).filter(|r| !r.is_empty()) {
            let (symbol, exponent) = split_exponent(run);
            for piece in split_symbols(symbol) {
                factors.push((piece, exponent * sign));
            }
        }
    }

    factors.sort();
    factors
        .into_iter()
        .map(|(symbol, exponent)| format!("{symbol}^{exponent}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn expand_superscripts(unit: &str) -> String {
    let mut out = String::new();
    let mut in_superscript = false;
    for c in unit.chars() {
        let plain = match c {
            '⁻' => Some('-'),
            '⁺' => Some('+'),
            '⁰' => Some('0'),
            '¹' => Some('1'),
            '²' => Some('2'),
            '³' => Some('3'),
            '⁴' => Some('4'),
            '⁵' => Some('5'),
            '⁶' => Some('6'),
            '⁷' => Some('7'),
            '⁸' => Some('8'),
            '⁹' => Some('9'),
            _ => None,
        };
        match plain {
            Some(p) => {
                if !in_superscript {
                    out.push('^');
                    in_superscript = true;
                }
                out.push(p);
            }
            None => {
                in_superscript = false;
                out.push(c);
            }
        }
    }
    out
}

/// `m^-1` into `("m", -1)`; a bare `m` into `("m", 1)`.
fn split_exponent(run: &str) -> (&str, i32) {
    match run.split_once('^') {
        Some((symbol, exponent)) => (symbol, exponent.parse().unwrap_or(1)),
        None => {
            // `m-1` and `m2`, written without the caret. Only when what
            // follows is entirely a number, so `Sb2Se3` is left alone.
            let split = run.find(|c: char| c == '-' || c.is_ascii_digit());
            match split {
                Some(at) if at > 0 && run[at..].parse::<i32>().is_ok() => {
                    (&run[..at], run[at..].parse().unwrap_or(1))
                }
                _ => (run, 1),
            }
        }
    }
}

/// `mK` into `["m", "K"]`; anything not wholly made of known symbols is left
/// as it was, so an unrecognised unit still compares with itself.
fn split_symbols(run: &str) -> Vec<String> {
    if run.is_empty() || SYMBOLS.contains(&run) {
        return vec![run.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = run;
    while !rest.is_empty() {
        let Some(symbol) = SYMBOLS.iter().find(|s| rest.starts_with(*s)) else {
            return vec![run.to_string()];
        };
        out.push((*symbol).to_string());
        rest = &rest[symbol.len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Claim {
        let found = claims(text);
        assert_eq!(found.len(), 1, "{text} -> {found:?}");
        found.into_iter().next().unwrap()
    }

    #[test]
    fn a_claim_is_a_label_a_number_and_a_unit() {
        let claim = one("κ = 0.037 W m⁻¹ K⁻¹ at 300 K.");
        assert_eq!(claim.label, "κ");
        assert_eq!(claim.value, 0.037);
        assert_eq!(claim.unit, "W m⁻¹ K⁻¹");
        assert_eq!(claim.text, "κ = 0.037 W m⁻¹ K⁻¹");
    }

    #[test]
    fn the_same_unit_spelled_three_ways_compares() {
        // The realistic failure this has to survive: nobody writes a unit the
        // same way twice, and a comparison that depended on them doing so
        // would almost never fire.
        let target = canonical_unit("W m⁻¹ K⁻¹");
        assert_eq!(canonical_unit("W/mK"), target);
        assert_eq!(canonical_unit("W/(m·K)"), target);
        assert_eq!(canonical_unit("W m^-1 K^-1"), target);
        assert_eq!(canonical_unit("W/(m K)"), target);
        // And a genuinely different unit does not.
        assert_ne!(canonical_unit("W m K"), target);
        assert_ne!(canonical_unit("J/mol/K"), target);
    }

    #[test]
    fn units_that_are_not_the_same_are_not_lined_up() {
        // Case carries meaning here: `m` is metres and `M` is molar, `s` is
        // seconds and `S` is siemens. Lowercasing would equate them silently.
        assert_ne!(canonical_unit("m"), canonical_unit("M"));
        assert_ne!(canonical_unit("s"), canonical_unit("S"));
        // A multi-letter symbol is not split into its letters.
        assert_eq!(canonical_unit("Pa"), "Pa^1");
        assert_eq!(canonical_unit("mol"), "mol^1");
        assert_eq!(canonical_unit("J/mol K"), canonical_unit("J mol⁻¹ K⁻¹"));
        // And something the table has never heard of still compares with
        // itself rather than being mangled.
        assert_eq!(
            canonical_unit("counts/frame"),
            canonical_unit("counts/frame")
        );
    }

    #[test]
    fn two_values_of_the_same_quantity_a_factor_apart_disagree() {
        // The step's proof, in one assertion.
        let here = one("κ = 0.037 W m⁻¹ K⁻¹");
        let there = one("κ = 0.37 W/mK");
        assert!(disagree(&here, &there));
        assert_eq!(factor(here.value, there.value).unwrap().round(), 10.0);
    }

    #[test]
    fn measurements_that_merely_differ_in_precision_do_not() {
        let a = one("κ = 0.037 W/mK");
        let b = one("κ = 0.039 W/mK");
        assert!(!disagree(&a, &b));
    }

    #[test]
    fn two_different_quantities_in_the_same_unit_are_not_a_disagreement() {
        // The failure that would make this feature worthless: every
        // temperature in the vault differs from every other, and none of that
        // is a contradiction.
        let onset = one("onset = 420 K");
        let melting = one("melting = 890 K");
        assert!(!disagree(&onset, &melting));
    }

    #[test]
    fn the_same_number_in_different_units_is_not_compared() {
        // Because we cannot tell that they are the same quantity, and
        // guessing would be inventing the fact the comparison rests on.
        let a = one("length = 3 m");
        let b = one("length = 300 cm");
        assert!(!disagree(&a, &b));
    }

    #[test]
    fn a_bare_number_only_compares_with_another_bare_number() {
        let a = one("ratio = 3");
        let b = one("ratio = 30");
        assert_eq!(a.unit_key, "");
        assert!(disagree(&a, &b));
        // But never with one that named its unit.
        let c = one("ratio = 30 %");
        assert!(!disagree(&a, &c));
    }

    #[test]
    fn a_range_is_not_a_claim() {
        // "Ramped 300–800 K" asserts a ramp, not a value, and comparing its
        // ends against another note's would be comparing two things nobody
        // said. Every dash people actually type.
        for text in [
            "range = 300-800 K",
            "range = 300–800 K",
            "range = 300 — 800 K",
            "range = 300 to 800 K",
        ] {
            assert!(claims(text).is_empty(), "{text} -> {:?}", claims(text));
        }
    }

    #[test]
    fn prose_numbers_without_a_label_are_left_alone() {
        // The restriction that makes the whole thing usable. A note is full of
        // numbers; only the ones written as an assertion are assertions.
        for text in [
            "Ramped to 800 K at 10 K/min under argon.",
            "The third run gave a cleaner baseline.",
            "See page 6 for the derivation.",
            "It sits 4% low at the top of the range.",
        ] {
            assert!(claims(text).is_empty(), "{text} -> {:?}", claims(text));
        }
    }

    #[test]
    fn code_and_syntax_are_not_claims() {
        // `==`, `:=` and `::` turn up in a research vault's code blocks, and
        // none of them is somebody claiming a measurement.
        assert!(claims("if x == 3 { }").is_empty());
        assert!(claims("let n := 42").is_empty());
        assert!(claims("std::f64 = 3").len() <= 1);
    }

    #[test]
    fn a_label_is_a_name_and_not_the_sentence_before_it() {
        // Two words back at most: a long run of prose ending in `=` is a
        // sentence, and its "label" would match nothing in any other note.
        let claim = one("The measured thermal conductivity = 0.037 W/mK");
        assert_eq!(claim.key, "measured thermal conductivity");

        // Punctuation ends a label outright.
        let claim = one("After the anneal, Cp = 120 J/mol K");
        assert_eq!(claim.label, "Cp");
    }

    #[test]
    fn the_written_out_form_is_read_too() {
        let claim = one("a thermal conductivity of 0.037 W/mK");
        assert_eq!(claim.key, "a thermal conductivity");
        assert_eq!(claim.value, 0.037);
        assert_eq!(canonical_unit(&claim.unit), canonical_unit("W m⁻¹ K⁻¹"));
    }

    #[test]
    fn scientific_notation_and_signs_are_read() {
        assert_eq!(one("n = 1e17 cm⁻³").value, 1e17);
        assert_eq!(one("n = 2.5E-3 mol").value, 2.5e-3);
        assert_eq!(one("offset = -4.2 K").value, -4.2);
        // A minus sign someone pasted from a paper, rather than typed.
        assert_eq!(one("offset = −4.2 K").value, -4.2);
    }

    #[test]
    fn a_sign_change_is_not_reported_as_a_ratio() {
        // "Twice as much as nothing" is not a ratio, and a value that changed
        // sign is a different kind of disagreement than arithmetic can speak
        // to. Both are silence rather than a wrong number.
        assert_eq!(factor(0.0, 3.0), None);
        assert_eq!(factor(-3.0, 3.0), None);
        assert_eq!(factor(-1.0, -10.0), Some(10.0));
    }

    #[test]
    fn several_claims_on_one_line_are_all_found() {
        let found = claims("κ = 0.037 W/mK, Cp = 120 J/mol K, n = 1e17 cm⁻³");
        assert_eq!(
            found.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
            ["κ", "cp", "n"]
        );
    }

    #[test]
    fn a_label_never_reads_across_a_line_break() {
        let found = claims("The anneal was long.\nCp = 120 J/mol K");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label, "Cp");
    }
}
