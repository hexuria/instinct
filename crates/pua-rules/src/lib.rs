//! `pua-rules`: data-driven cue rules over canonical tokens (spec §4.4).
//!
//! A [`RuleSet`] is compiled once from a [`RuleSetSpec`] (TOML in a pack's `data/`). Patterns
//! are literal canonical tokens plus two slots, `{word}` and `{gerund}`; there is **no regex**
//! in rule data. Matching is keyed by token (ADR 0005), so it is token-boundary by
//! construction: `stop` never fires inside `stopwatch`.
//!
//! [`RuleSet::score`] finds every match over the free tokens of one sentence, then applies, in
//! order:
//!
//! 1. **Specificity.** A match whose token range lies strictly inside another match's range is
//!    suppressed. That is how object scope works: `stop {gerund}` (steer) covers `stop using X`
//!    and suppresses the bare `stop` (interrupt) inside it.
//! 2. **Negation.** A negator phrase inside the `negation_window` tokens before the match (same
//!    sentence) cancels rules that forbid `negation`: "don't stop", "no need to stop".
//! 3. **Question damper.** In a sentence ending with `?` (outside protected spans) the weight is
//!    halved, unless the rule requires `question`.
//! 4. **Repairs.** Tokens the lexicon repaired (`stpo` → `stop`) match as their term and their
//!    repair penalty is subtracted from the weight.
//!
//! Per class, the counted weights combine by the class's [`ScorerKind`]: `max` (critical set:
//! edits outside the winning match and its negation window cannot change the class score) or
//! `sum` (saturating at 1000). Conflicting classes are all kept; the decide stage resolves them
//! by margin. Tokens in protected spans and confusable tokens never match a literal.
//!
//! ```
//! use pua_core::ScorerKind;
//! use pua_rules::{ClassSpec, Repairs, RuleSet, RuleSetSpec, RuleSpec};
//! use pua_text::{NormalizeConfig, normalize};
//!
//! let rule = |id: &str, class: &str, pattern: &str, w| RuleSpec {
//!     id: id.into(), class: class.into(), pattern: pattern.into(), weight_millis: w,
//!     requires: vec![], forbids: vec!["negation".into()], version: 1,
//! };
//! let spec = RuleSetSpec {
//!     classes: vec![
//!         ClassSpec { name: "interrupt".into(), scorer: ScorerKind::Max },
//!         ClassSpec { name: "steer".into(), scorer: ScorerKind::Max },
//!     ],
//!     negators: vec!["don't".into()],
//!     negation_window: 3,
//!     rules: vec![
//!         rule("interrupt.stop.v1", "interrupt", "stop", 700),
//!         rule("steer.stop_gerund.v1", "steer", "stop {gerund}", 650),
//!     ],
//! };
//! let rules = RuleSet::new(&spec, NormalizeConfig::default())?;
//! let score = |s: &str| {
//!     let n = normalize(s, NormalizeConfig::default()).unwrap();
//!     rules.score(&n, &Repairs::none()).unwrap().scores().iter().map(|c| c.get()).collect::<Vec<_>>()
//! };
//! assert_eq!(score("STOP now"), [700, 0]);
//! assert_eq!(score("stop using postgres"), [0, 650]);
//! assert_eq!(score("don't stop"), [0, 0]);
//! assert_eq!(score("should I stop?"), [350, 0]);
//! assert_eq!(score("the `stop` command"), [0, 0]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod spec;

use core::fmt;
use core::ops::Range;
use std::collections::{BTreeMap, BTreeSet};

use pua_core::{Confidence, Millis, ScorerKind, Span, StageKind, TrailRecord};
use pua_lexicon::{Lexicon, Lookup, MatchKind};
use pua_text::{NormalizeConfig, Normalized, normalize};

pub use crate::spec::{
    ClassSpec, DEFAULT_NEGATION_WINDOW, MAX_NEGATION_WINDOW, MAX_NEGATOR_TOKENS, MAX_PATTERN_ITEMS,
    RuleSetSpec, RuleSpec, RulesError,
};

/// Shortest token (in chars) a `{gerund}` slot accepts.
pub const MIN_GERUND_CHARS: usize = 5;

/// Why scoring was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreError {
    /// The text was canonicalized with a different config than the rule literals.
    ConfigMismatch {
        /// The rule set's config.
        rules: NormalizeConfig,
        /// The text's config.
        text: NormalizeConfig,
    },
}

impl fmt::Display for ScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigMismatch { rules, text } => write!(
                f,
                "text normalized with {} but rules expect {}",
                text.fingerprint(),
                rules.fingerprint()
            ),
        }
    }
}

impl std::error::Error for ScoreError {}

/// Index of a class, in declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(u16);

impl ClassId {
    /// Position in the declared class list.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index of a rule in a compiled [`RuleSet`] (rules are sorted by id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleRef(u32);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Lit(Box<str>),
    Word,
    Gerund,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Ctx {
    negation: bool,
    question: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    id: Box<str>,
    class: ClassId,
    items: Box<[Item]>,
    weight: Confidence,
    requires: Ctx,
    forbids: Ctx,
    version: u32,
}

/// Token texts substituted by lexicon repairs, with their penalties.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repairs {
    by_token: BTreeMap<usize, (Box<str>, Confidence)>,
}

impl Repairs {
    /// No repairs: rules see the canonical tokens as they are.
    pub fn none() -> Self {
        Self::default()
    }

    /// Single-token typo repairs from a lexicon lookup over the same text.
    pub fn from_lookup(lexicon: &Lexicon, lookup: &Lookup) -> Self {
        let mut by_token = BTreeMap::new();
        for h in lookup.hits() {
            if let MatchKind::Repaired { penalty, .. } = h.kind()
                && h.tokens().len() == 1
            {
                by_token.insert(h.tokens().start, (lexicon.term(h.entry()).into(), penalty));
            }
        }
        Self { by_token }
    }
}

/// What happened to a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchStatus {
    /// Counted toward its class.
    Counted,
    /// Inside a longer match; not counted.
    Suppressed,
    /// A negator precedes it and the rule forbids negation.
    Negated,
    /// A required context is missing, or a forbidden non-negation context is present.
    ContextUnmet,
}

impl MatchStatus {
    /// Stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Counted => "counted",
            Self::Suppressed => "suppressed",
            Self::Negated => "negated",
            Self::ContextUnmet => "context-unmet",
        }
    }
}

/// One rule match.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleMatch {
    rule: RuleRef,
    class: ClassId,
    tokens: (usize, usize),
    original: Span,
    status: MatchStatus,
    question: bool,
    penalty: Confidence,
    effective: Confidence,
}

impl RuleMatch {
    /// The rule.
    pub fn rule(&self) -> RuleRef {
        self.rule
    }
    /// Its class.
    pub fn class(&self) -> ClassId {
        self.class
    }
    /// Token range covered.
    pub fn tokens(&self) -> Range<usize> {
        self.tokens.0..self.tokens.1
    }
    /// Span in the original text.
    pub fn original(&self) -> Span {
        self.original
    }
    /// Counted, suppressed, negated or context-unmet.
    pub fn status(&self) -> MatchStatus {
        self.status
    }
    /// Whether the sentence is a question.
    pub fn question(&self) -> bool {
        self.question
    }
    /// Total repair penalty of the matched tokens.
    pub fn repair_penalty(&self) -> Confidence {
        self.penalty
    }
    /// Weight after damper and penalties; zero unless counted.
    pub fn effective(&self) -> Confidence {
        self.effective
    }
}

/// Result of [`RuleSet::score`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleScores {
    scores: Vec<Confidence>,
    matches: Vec<RuleMatch>,
}

impl RuleScores {
    /// Per-class score in declaration order.
    pub fn scores(&self) -> &[Confidence] {
        &self.scores
    }
    /// One class's score.
    pub fn score(&self, class: ClassId) -> Confidence {
        self.scores
            .get(class.index())
            .copied()
            .unwrap_or(Confidence::ZERO)
    }
    /// All matches, ordered by (start token, rule id).
    pub fn matches(&self) -> &[RuleMatch] {
        &self.matches
    }
}

/// A validated, compiled rule set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    config: NormalizeConfig,
    classes: Vec<(Box<str>, ScorerKind)>,
    rules: Vec<Rule>,
    /// First literal → (rule, its offset in the pattern).
    index: BTreeMap<Box<str>, Vec<(RuleRef, usize)>>,
    negators: Vec<Box<[Box<str>]>>,
    window: u8,
}

/// Canonical free, non-confusable tokens of `s` if `s` is exactly those tokens joined by
/// single spaces.
fn canonical_words(s: &str, config: NormalizeConfig) -> Option<Vec<Box<str>>> {
    let n = normalize(s, config).ok()?;
    let mut out = Vec::new();
    for t in n.tokens() {
        if !t.is_free() || t.confusable().is_some() {
            return None;
        }
        out.push(Box::<str>::from(n.token_text(t)));
    }
    (!out.is_empty() && out.join(" ") == s).then_some(out)
}

fn parse_ctx(id: &str, list: &[String]) -> Result<Ctx, RulesError> {
    let mut c = Ctx::default();
    for name in list {
        let slot = match name.as_str() {
            "negation" => &mut c.negation,
            "question" => &mut c.question,
            _ => {
                return Err(RulesError::UnknownContext {
                    id: id.into(),
                    context: name.clone(),
                });
            }
        };
        if *slot {
            return Err(RulesError::DuplicateContext {
                id: id.into(),
                context: name.clone(),
            });
        }
        *slot = true;
    }
    Ok(c)
}

fn compile_rule(
    r: &RuleSpec,
    classes: &[(Box<str>, ScorerKind)],
    config: NormalizeConfig,
) -> Result<Rule, RulesError> {
    let id = || r.id.clone();
    let class = classes
        .iter()
        .position(|(n, _)| **n == *r.class)
        .and_then(|i| u16::try_from(i).ok())
        .map(ClassId)
        .ok_or_else(|| RulesError::UnknownClass {
            id: id(),
            class: r.class.clone(),
        })?;
    let weight = (1..=1000)
        .contains(&r.weight_millis)
        .then(|| Confidence::new(r.weight_millis).ok())
        .flatten()
        .ok_or(RulesError::WeightOutOfRange {
            id: id(),
            weight: r.weight_millis,
        })?;
    if r.version == 0 {
        return Err(RulesError::ZeroVersion { id: id() });
    }
    if r.pattern.is_empty() {
        return Err(RulesError::EmptyPattern { id: id() });
    }
    let raw: Vec<&str> = r.pattern.split(' ').collect();
    if raw.len() > MAX_PATTERN_ITEMS {
        return Err(RulesError::PatternTooLong {
            id: id(),
            items: raw.len(),
        });
    }
    let mut items = Vec::with_capacity(raw.len());
    for w in raw {
        let item = match w {
            "{word}" => Item::Word,
            "{gerund}" => Item::Gerund,
            _ if w.starts_with('{') || w.ends_with('}') => {
                return Err(RulesError::UnknownSlot {
                    id: id(),
                    slot: w.into(),
                });
            }
            _ => match canonical_words(w, config).as_deref() {
                Some([one]) => Item::Lit(one.clone()),
                _ => {
                    return Err(RulesError::NonCanonicalLiteral {
                        id: id(),
                        literal: w.into(),
                    });
                }
            },
        };
        items.push(item);
    }
    if !items.iter().any(|i| matches!(i, Item::Lit(_))) {
        return Err(RulesError::NoLiteral { id: id() });
    }
    let requires = parse_ctx(&r.id, &r.requires)?;
    let forbids = parse_ctx(&r.id, &r.forbids)?;
    for (name, both) in [
        ("negation", requires.negation && forbids.negation),
        ("question", requires.question && forbids.question),
    ] {
        if both {
            return Err(RulesError::ContextConflict {
                id: id(),
                context: name.into(),
            });
        }
    }
    Ok(Rule {
        id: r.id.as_str().into(),
        class,
        items: items.into_boxed_slice(),
        weight,
        requires,
        forbids,
        version: r.version,
    })
}

impl RuleSet {
    /// Validates and compiles `spec`; literals and negators are checked against `config`.
    ///
    /// # Errors
    /// The first [`RulesError`] found: classes, then negators and window, then rules in order.
    pub fn new(spec: &RuleSetSpec, config: NormalizeConfig) -> Result<Self, RulesError> {
        if spec.classes.is_empty() {
            return Err(RulesError::NoClasses);
        }
        let mut classes: Vec<(Box<str>, ScorerKind)> = Vec::new();
        for (index, c) in spec.classes.iter().enumerate() {
            if c.name.is_empty() {
                return Err(RulesError::EmptyClassName { index });
            }
            if classes.iter().any(|(n, _)| **n == *c.name) {
                return Err(RulesError::DuplicateClass {
                    name: c.name.clone(),
                });
            }
            classes.push((c.name.as_str().into(), c.scorer));
        }
        let mut negators = BTreeSet::new();
        for (index, n) in spec.negators.iter().enumerate() {
            let words = canonical_words(n, config)
                .filter(|w| w.len() <= MAX_NEGATOR_TOKENS)
                .ok_or(RulesError::NonCanonicalNegator { index })?;
            if !negators.insert(words.into_boxed_slice()) {
                return Err(RulesError::DuplicateNegator { index });
            }
        }
        if !(1..=MAX_NEGATION_WINDOW).contains(&spec.negation_window) {
            return Err(RulesError::NegationWindowOutOfRange {
                window: spec.negation_window,
            });
        }
        if spec.rules.is_empty() {
            return Err(RulesError::NoRules);
        }
        let mut ids = BTreeSet::new();
        let mut rules = Vec::with_capacity(spec.rules.len());
        for (index, r) in spec.rules.iter().enumerate() {
            if r.id.is_empty() {
                return Err(RulesError::EmptyRuleId { index });
            }
            if !ids.insert(r.id.as_str()) {
                return Err(RulesError::DuplicateRuleId { id: r.id.clone() });
            }
            rules.push(compile_rule(r, &classes, config)?);
        }
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        let mut index: BTreeMap<Box<str>, Vec<(RuleRef, usize)>> = BTreeMap::new();
        for (i, r) in rules.iter().enumerate() {
            if let Some((off, Item::Lit(w))) = r
                .items
                .iter()
                .enumerate()
                .find(|(_, it)| matches!(it, Item::Lit(_)))
            {
                index
                    .entry(w.clone())
                    .or_default()
                    .push((RuleRef(u32::try_from(i).unwrap_or(u32::MAX)), off));
            }
        }
        Ok(Self {
            config,
            classes,
            rules,
            index,
            negators: negators.into_iter().collect(),
            window: spec.negation_window,
        })
    }

    /// The config literals were validated against.
    pub fn config(&self) -> NormalizeConfig {
        self.config
    }

    /// Number of classes.
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Looks up a class by name.
    pub fn class_id(&self, name: &str) -> Option<ClassId> {
        self.classes
            .iter()
            .position(|(n, _)| **n == *name)
            .and_then(|i| u16::try_from(i).ok())
            .map(ClassId)
    }

    /// A class's name (`""` for an id from another rule set).
    pub fn class_name(&self, class: ClassId) -> &str {
        self.classes.get(class.index()).map_or("", |(n, _)| n)
    }

    /// A class's scorer.
    pub fn scorer(&self, class: ClassId) -> Option<ScorerKind> {
        self.classes.get(class.index()).map(|(_, k)| *k)
    }

    /// A rule's id (`""` for a ref from another rule set).
    pub fn rule_id(&self, rule: RuleRef) -> &str {
        self.rules.get(rule.0 as usize).map_or("", |r| &r.id)
    }

    /// Canonical bytes for `DataVersion`: config, classes, negators, window and every rule
    /// (sorted by id) with pattern, weight, contexts and version.
    pub fn fingerprint(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut put = |b: &[u8]| {
            out.extend_from_slice(&(b.len() as u64).to_le_bytes());
            out.extend_from_slice(b);
        };
        put(b"pua-rules-v1");
        put(self.config.fingerprint().as_bytes());
        for (n, k) in &self.classes {
            put(n.as_bytes());
            put(k.name().as_bytes());
        }
        put(b"negators");
        for n in &self.negators {
            put(n.join(" ").as_bytes());
        }
        put(&[self.window]);
        for r in &self.rules {
            put(r.id.as_bytes());
            put(self.class_name(r.class).as_bytes());
            for it in &r.items {
                match it {
                    Item::Lit(w) => put(w.as_bytes()),
                    Item::Word => put(b"{word}"),
                    Item::Gerund => put(b"{gerund}"),
                }
            }
            put(&r.weight.get().to_le_bytes());
            put(&[
                u8::from(r.requires.negation),
                u8::from(r.requires.question),
                u8::from(r.forbids.negation),
                u8::from(r.forbids.question),
            ]);
            put(&r.version.to_le_bytes());
        }
        out
    }

    /// Scores `text` (see the crate docs for the order of effects).
    ///
    /// # Errors
    /// [`ScoreError::ConfigMismatch`] when `text` was normalized with a different config.
    pub fn score(
        &self,
        text: &Normalized<'_>,
        repairs: &Repairs,
    ) -> Result<RuleScores, ScoreError> {
        if text.config() != self.config {
            return Err(ScoreError::ConfigMismatch {
                rules: self.config,
                text: text.config(),
            });
        }
        let view = View::new(text, repairs);
        let mut raw: Vec<(usize, usize, RuleRef)> = Vec::new();
        for (i, w) in view.words.iter().enumerate() {
            let Some(w) = w else { continue };
            let Some(cands) = self.index.get(*w) else {
                continue;
            };
            for &(rule, off) in cands {
                let Some(start) = i.checked_sub(off) else {
                    continue;
                };
                let r = &self.rules[rule.0 as usize];
                if view.matches(&r.items, start) {
                    raw.push((start, start + r.items.len(), rule));
                }
            }
        }
        raw.sort_unstable_by_key(|&(s, e, r)| (s, r, e));
        let mut matches = Vec::with_capacity(raw.len());
        let mut scores = vec![Confidence::ZERO; self.classes.len()];
        for &(s, e, rule) in &raw {
            let r = &self.rules[rule.0 as usize];
            // A containing match starts at most MAX_PATTERN_ITEMS tokens earlier: only that
            // window of the start-sorted list is examined (bounded work per match).
            let lo = raw.partition_point(|&(s2, _, _)| s2 < s.saturating_sub(MAX_PATTERN_ITEMS));
            let hi = raw.partition_point(|&(s2, _, _)| s2 <= s);
            let suppressed = raw[lo..hi]
                .iter()
                .any(|&(s2, e2, _)| e <= e2 && (s2, e2) != (s, e));
            let negated = self.negated(&view, s);
            let question = view.question(s);
            let status = if suppressed {
                MatchStatus::Suppressed
            } else if negated && r.forbids.negation {
                MatchStatus::Negated
            } else if (r.requires.negation && !negated)
                || (r.requires.question && !question)
                || (r.forbids.question && question)
            {
                MatchStatus::ContextUnmet
            } else {
                MatchStatus::Counted
            };
            let penalty = (s..e).fold(Confidence::ZERO, |acc, t| {
                acc.saturating_add(view.penalty(t))
            });
            let effective = if status == MatchStatus::Counted {
                let w = if question && !r.requires.question {
                    r.weight.halved()
                } else {
                    r.weight
                };
                w.saturating_sub(penalty)
            } else {
                Confidence::ZERO
            };
            if status == MatchStatus::Counted {
                let slot = &mut scores[r.class.index()];
                *slot = match self.classes[r.class.index()].1 {
                    ScorerKind::Max => (*slot).max(effective),
                    ScorerKind::Sum => slot.saturating_add(effective),
                };
            }
            matches.push(RuleMatch {
                rule,
                class: r.class,
                tokens: (s, e),
                original: view.span(s, e),
                status,
                question,
                penalty,
                effective,
            });
        }
        Ok(RuleScores { scores, matches })
    }

    fn negated(&self, view: &View<'_, '_>, start: usize) -> bool {
        let sentence = view.sentence[start];
        let lo = start.saturating_sub(usize::from(self.window));
        // Window: tokens lo..start of the same sentence.
        let lo = (lo..start)
            .find(|&t| view.sentence[t] == sentence)
            .unwrap_or(start);
        self.negators.iter().any(|neg| {
            (lo..start).any(|b| {
                b + neg.len() <= start
                    && neg
                        .iter()
                        .enumerate()
                        .all(|(k, w)| view.raw[b + k] == Some(&**w))
            })
        })
    }

    /// Trail records for every match, in match order.
    pub fn trail(&self, scores: &RuleScores) -> Vec<TrailRecord> {
        scores
            .matches
            .iter()
            .map(|m| {
                let mut rec = TrailRecord::new(
                    StageKind::Rules,
                    format!(
                        "{} {}{}",
                        m.status.name(),
                        self.class_name(m.class),
                        if m.question { " (question)" } else { "" }
                    ),
                )
                .rule(self.rule_id(m.rule))
                .span(m.original)
                .millis(Millis::new(m.effective.get()).unwrap_or(Millis::ZERO));
                if let Some(k) = self.scorer(m.class) {
                    rec = rec.scorer(k);
                }
                rec
            })
            .collect()
    }
}

/// Per-token view of a text for matching.
struct View<'n, 'r> {
    text: &'n Normalized<'n>,
    /// Effective word (repair applied) for free, non-confusable tokens.
    words: Vec<Option<&'n str>>,
    /// Literal canonical text for free tokens (negators match this, never repairs).
    raw: Vec<Option<&'n str>>,
    /// Free tokens (slots accept confusable ones too).
    free: Vec<bool>,
    sentence: Vec<u32>,
    questions: Vec<bool>,
    repairs: &'r Repairs,
}

impl<'n, 'r> View<'n, 'r>
where
    'r: 'n,
{
    fn new(text: &'n Normalized<'n>, repairs: &'r Repairs) -> Self {
        let toks = text.tokens();
        let mut words = Vec::with_capacity(toks.len());
        let mut raw = Vec::with_capacity(toks.len());
        for (i, t) in toks.iter().enumerate() {
            let plain = t.is_free().then(|| text.token_text(t));
            raw.push(plain);
            let eff = if t.is_free() && t.confusable().is_none() {
                Some(
                    repairs
                        .by_token
                        .get(&i)
                        .map_or(text.token_text(t), |(w, _)| &**w),
                )
            } else {
                None
            };
            words.push(eff);
        }
        Self {
            text,
            words,
            raw,
            free: toks.iter().map(pua_text::Token::is_free).collect(),
            sentence: toks.iter().map(pua_text::Token::sentence).collect(),
            questions: question_sentences(text),
            repairs,
        }
    }

    fn matches(&self, items: &[Item], start: usize) -> bool {
        let Some(&sentence) = self.sentence.get(start) else {
            return false;
        };
        items.iter().enumerate().all(|(k, it)| {
            let t = start + k;
            if self.sentence.get(t) != Some(&sentence) || !self.free[t] {
                return false;
            }
            match it {
                Item::Lit(w) => self.words[t] == Some(&**w),
                Item::Word => true,
                Item::Gerund => self.words[t]
                    .is_some_and(|w| w.ends_with("ing") && w.chars().count() >= MIN_GERUND_CHARS),
            }
        })
    }

    fn question(&self, token: usize) -> bool {
        self.questions
            .get(self.sentence[token] as usize)
            .copied()
            .unwrap_or(false)
    }

    fn penalty(&self, token: usize) -> Confidence {
        self.repairs
            .by_token
            .get(&token)
            .map_or(Confidence::ZERO, |(_, p)| *p)
    }

    fn span(&self, s: usize, e: usize) -> Span {
        let toks = self.text.tokens();
        toks[s].original().cover(toks[e - 1].original())
    }
}

/// For each sentence: does a `?` follow its last token (before the next sentence), outside
/// protected spans?
fn question_sentences(text: &Normalized<'_>) -> Vec<bool> {
    let toks = text.tokens();
    let canon = text.canonical();
    let count = text.sentence_count() as usize;
    let mut out = vec![false; count.max(1)];
    let protected: Vec<Range<usize>> = text
        .protected()
        .iter()
        .map(pua_text::Protected::canonical_range)
        .collect();
    for (i, t) in toks.iter().enumerate() {
        let next_is_same = toks
            .get(i + 1)
            .is_some_and(|n| n.sentence() == t.sentence());
        if next_is_same {
            continue;
        }
        let from = t.canonical_range().end;
        let to = toks
            .get(i + 1)
            .map_or(canon.len(), |n| n.canonical_range().start);
        let q = canon.get(from..to).is_some_and(|tail| {
            tail.char_indices()
                .any(|(k, c)| c == '?' && !protected.iter().any(|p| p.contains(&(from + k))))
        });
        if let Some(slot) = out.get_mut(t.sentence() as usize) {
            *slot = q;
        }
    }
    out
}

#[cfg(test)]
mod tests;
