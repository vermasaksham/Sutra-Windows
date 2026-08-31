//! Optional AI assistance, off by default, behind an interface.
//!
//! # The rules this module exists to enforce
//!
//! The brief is specific, and each rule is structural here rather than a
//! promise in a prompt:
//!
//! 1. **It may not write to a file.** Nothing in this module can. `vault` is
//!    the only module that writes, this one does not import it, and an
//!    [`Assistant`] returns a [`Suggestion`] — a value with no path, no id and
//!    no way to reach the disk. Accepting a suggestion happens through the
//!    same ordinary commands a person's own typing goes through, so a
//!    generated sentence and a typed one take exactly the same road to a file.
//! 2. **It may not produce a citation the vault does not have.** Enforced by
//!    [`crate::citations::drop_unknown`] on every response, not by asking the
//!    model nicely. The prompt says so too, but the prompt is not the
//!    guarantee.
//! 3. **Off by default.** [`Off`] is the default assistant and every method on
//!    it declines. Being switched off is a type rather than a flag someone can
//!    forget to check.
//! 4. **Marked as generated.** A [`Suggestion`] carries the model that wrote
//!    it, and the UI renders it in a way that cannot be mistaken for the
//!    note's own text.
//!
//! # Why it is last, and optional
//!
//! Every suggestion in Sutra has a deterministic implementation first —
//! related notes, duplicates, differing numbers, tag merges. Those can show
//! their working: *these two notes share four terms and a tag*. A model's
//! guess cannot. For a vault holding years of research, explainable beats
//! clever, and the failure this app most needs to avoid is inventing
//! something that reads like a finding.

use crate::citations;
use crate::error::{Result, SutraError};
use serde::{Deserialize, Serialize};

/// What the assistant is being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Task {
    /// A few sentences saying what the note is about.
    Summarise,
    /// Tags for the note, preferring ones the vault already uses.
    Tags,
    /// Questions the note raises but does not answer.
    Questions,
}

impl Task {
    fn instruction(self) -> &'static str {
        match self {
            Self::Summarise => {
                "Summarise this note in at most three sentences. Write in the note's own \
                 register, as a colleague would, not as a report about it. Do not begin \
                 with \"This note\"."
            }
            Self::Tags => {
                "Suggest up to five tags for this note, one per line, with no bullets, no \
                 hash and no other text. Strongly prefer tags the vault already uses, \
                 listed below; propose a new one only where nothing existing fits. Use \
                 slashes for hierarchy, as in method/xrd."
            }
            Self::Questions => {
                "List up to four questions this note raises and does not answer, one per \
                 line, with no bullets and no other text. Ask about what the note itself \
                 leaves open — not general questions about the subject."
            }
        }
    }

    /// How hard to think about it.
    ///
    /// Tags are a classification against a list that is already in the prompt;
    /// the other two are reading. Neither is worth the latency of a long think
    /// in an app where this sits beside the note being written.
    fn effort(self) -> &'static str {
        match self {
            Self::Tags => "low",
            Self::Summarise | Self::Questions => "medium",
        }
    }
}

/// Everything the assistant is given. Nothing else leaves the machine.
///
/// One note, and the vault's tag list when tags are being asked for. Not the
/// vault, not the neighbouring notes, not the file paths — a research vault is
/// years of unpublished work, and the amount of it that goes to a third party
/// should be the minimum the question needs and visible in one struct.
#[derive(Debug, Clone)]
pub struct Ask {
    pub task: Task,
    pub title: String,
    pub body: String,
    /// Tags the vault already uses, most-used first. Empty for other tasks.
    pub vault_tags: Vec<String>,
    /// Source note ids the vault holds. Anything cited outside this is removed.
    pub known_sources: Vec<String>,
}

/// What came back: a draft, not an action.
///
/// Called a draft rather than a suggestion because that is what it is — text
/// that exists only in this reply until a person accepts it, at which point it
/// travels the same road to disk as anything they typed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub task: Task,
    /// The prose, for [`Task::Summarise`].
    pub text: String,
    /// One line each, for [`Task::Tags`] and [`Task::Questions`].
    pub lines: Vec<String>,
    /// Which model wrote it. Shown, always: a suggestion whose author is not
    /// on screen is one that can be mistaken for the note.
    pub model: String,
    /// References it invented, removed before this was returned. Surfaced
    /// rather than swallowed — a model that fabricates a citation once will do
    /// it again, and the person deciding whether to keep using it should know.
    pub removed_citations: Vec<String>,
}

/// Something that can be asked, and can do nothing else.
///
/// A trait rather than a concrete client so the app depends on the shape of
/// the question and not on who answers it. Swapping in a local model, or
/// nothing at all, changes this file and no other.
pub trait Assistant: Send + Sync {
    /// What to call it on screen.
    fn label(&self) -> String;
    fn respond(&self, ask: &Ask) -> Result<Draft>;
}

/// The default. Declines everything.
///
/// Off is a type, not a boolean somebody has to remember to test. A build with
/// no key configured cannot reach the network from here, because the object
/// that could is never constructed.
pub struct Off;

impl Assistant for Off {
    fn label(&self) -> String {
        "off".into()
    }

    fn respond(&self, _ask: &Ask) -> Result<Draft> {
        Err(SutraError::Ai(
            "AI assistance is switched off. Turn it on in settings if you want it.".into(),
        ))
    }
}

/// Sutra's default model.
///
/// Claude Opus 5. Thinking is adaptive on it by default, so the parameter is
/// left off and depth is steered with `effort` per task instead.
pub const DEFAULT_MODEL: &str = "claude-opus-5";

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const VERSION: &str = "2023-06-01";

/// Refusal fallbacks, in the scalar form that routes by category.
///
/// Opt-in, and opted into: without it a declined request simply stops, which
/// in a notes app reads as the feature being broken rather than as a decision.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Short answers, all three of them. Well under any HTTP timeout, so these
/// requests do not need streaming.
const MAX_TOKENS: u32 = 2000;

/// Claude over its HTTP API.
pub struct Claude {
    key: String,
    model: String,
}

impl Claude {
    pub fn new(key: String, model: String) -> Self {
        let model = if model.trim().is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            model
        };
        Self { key, model }
    }
}

impl Assistant for Claude {
    fn label(&self) -> String {
        self.model.clone()
    }

    fn respond(&self, ask: &Ask) -> Result<Draft> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "output_config": { "effort": ask.task.effort() },
            "fallbacks": "default",
            "system": system_prompt(ask),
            "messages": [{ "role": "user", "content": user_prompt(ask) }],
        });

        let response = ureq::post(ENDPOINT)
            .header("content-type", "application/json")
            .header("x-api-key", &self.key)
            .header("anthropic-version", VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .send_json(&body)
            .map_err(|e| SutraError::Ai(explain(&e)))?
            .body_mut()
            .read_json::<serde_json::Value>()
            .map_err(|e| SutraError::Ai(format!("could not read the reply: {e}")))?;

        // A refusal arrives as a 200 with nothing usable in it. Reported as
        // itself rather than as an empty suggestion, which would look like the
        // model having no opinion.
        if response["stop_reason"] == "refusal" {
            return Err(SutraError::Ai(
                "The model declined to answer this one. Nothing has been changed.".into(),
            ));
        }

        let text = response["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        // Which model actually answered, not which was asked for: a fallback
        // may have served this turn, and the label on a suggestion should say
        // who wrote it.
        let model = response["model"]
            .as_str()
            .unwrap_or(&self.model)
            .to_string();

        Ok(finish(ask, &text, model))
    }
}

/// Turn a model's reply into a draft, with the citation rule applied.
///
/// Separate from the transport so it can be tested without a network, and so
/// any future assistant gets the same treatment by construction rather than by
/// remembering to.
pub fn finish(ask: &Ask, text: &str, model: String) -> Draft {
    let (text, removed_citations) = citations::drop_unknown(text, &ask.known_sources);
    let lines = match ask.task {
        Task::Summarise => Vec::new(),
        Task::Tags | Task::Questions => text
            .lines()
            .map(|l| l.trim().trim_start_matches(['-', '*', '#', '•']).trim())
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
    };
    Draft {
        task: ask.task,
        text: text.trim().to_string(),
        lines,
        model,
        removed_citations,
    }
}

fn system_prompt(ask: &Ask) -> String {
    let mut prompt = String::from(
        "You are helping with a research vault of markdown notes. You are a suggestion \
         and nothing else: what you write is shown to the researcher marked as generated \
         and is discarded unless they accept it. You never edit their notes.\n\n\
         Two rules matter more than being helpful. Do not invent facts, findings, \
         numbers or sources — if the note does not say something, it is not known. And \
         do not write a citation of any kind; citations in this vault name notes that \
         exist in it, and one you compose would be removed before the researcher saw \
         it.\n\n\
         Answer with the requested content only. No preamble, no offer to help further, \
         no explanation of what you did.\n\n",
    );
    prompt.push_str(ask.task.instruction());
    prompt
}

fn user_prompt(ask: &Ask) -> String {
    let mut prompt = format!("Title: {}\n\n{}\n", ask.title, ask.body);
    if ask.task == Task::Tags && !ask.vault_tags.is_empty() {
        prompt.push_str("\nTags this vault already uses, most-used first:\n");
        for tag in ask.vault_tags.iter().take(60) {
            prompt.push_str(tag);
            prompt.push('\n');
        }
    }
    prompt
}

/// A transport failure, as a sentence someone can act on.
fn explain(error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(401) => {
            "The API key was refused. Check it in settings.".to_string()
        }
        ureq::Error::StatusCode(429) => "Rate limited. Wait a moment and ask again.".to_string(),
        ureq::Error::StatusCode(code) if *code >= 500 => {
            format!("The API is having trouble ({code}). Nothing has been changed.")
        }
        ureq::Error::StatusCode(code) => format!("The request was rejected ({code})."),
        other => format!("Could not reach the API: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(task: Task) -> Ask {
        Ask {
            task,
            title: "Sb2Se3 Cp".into(),
            body: "Cp fitted over 300-800 K.".into(),
            vault_tags: vec!["sb2se3".into(), "method/dsc".into()],
            known_sources: vec!["01HQ3M8K2P00000000000000A1".into()],
        }
    }

    /// An assistant that answers with whatever the test wants, so every rule
    /// below is exercised without a network.
    struct Fixed(&'static str);

    impl Assistant for Fixed {
        fn label(&self) -> String {
            "fixed".into()
        }
        fn respond(&self, ask: &Ask) -> Result<Draft> {
            Ok(finish(ask, self.0, "fixed".into()))
        }
    }

    #[test]
    fn off_is_the_default_and_declines_everything() {
        // Not a flag someone has to remember to check — a type. A build where
        // assistance is off never constructs the object that could reach the
        // network.
        for task in [Task::Summarise, Task::Tags, Task::Questions] {
            let refused = Off.respond(&ask(task));
            assert!(refused.is_err());
            assert!(
                refused.unwrap_err().to_string().contains("switched off"),
                "the reason should say what to do about it"
            );
        }
    }

    #[test]
    fn a_citation_the_vault_does_not_have_is_removed_and_reported() {
        // The rule that matters most. A fabricated citation is the one output
        // that looks exactly like provenance and is not, and this is enforced
        // in code rather than asked for in the prompt.
        let draft = Fixed(
            "Boundary scattering dominates [@01HQ3M8K2PMADEUPMADEUPMA], \
             as the fitted polynomial shows [@01HQ3M8K2P00000000000000A1].",
        )
        .respond(&ask(Task::Summarise))
        .unwrap();

        assert!(!draft.text.contains("MADEUP"), "{}", draft.text);
        assert!(
            draft.text.contains("[@01HQ3M8K2P00000000000000A1]"),
            "a citation the vault does have is left alone: {}",
            draft.text
        );
        assert_eq!(draft.removed_citations, ["01HQ3M8K2PMADEUPMADEUPMA"]);
    }

    #[test]
    fn every_invented_citation_goes_even_when_there_are_several() {
        let draft = Fixed("One [@AAAA1111], two [@BBBB2222], three [@AAAA1111].")
            .respond(&ask(Task::Summarise))
            .unwrap();
        assert!(!draft.text.contains("[@"), "{}", draft.text);
        assert_eq!(draft.removed_citations, ["AAAA1111", "BBBB2222"]);
    }

    #[test]
    fn a_draft_says_which_model_wrote_it() {
        // Shown always. A suggestion whose author is not on screen is one that
        // can be mistaken for the note's own text.
        let draft = Fixed("Anything.").respond(&ask(Task::Summarise)).unwrap();
        assert_eq!(draft.model, "fixed");
    }

    #[test]
    fn a_list_answer_is_read_a_line_at_a_time_and_tidied() {
        // Models bullet things however they were feeling. The prompt asks for
        // bare lines; this makes it not matter.
        let draft = Fixed("- sb2se3\n* method/dsc\n\n  #thermal  \n")
            .respond(&ask(Task::Tags))
            .unwrap();
        assert_eq!(draft.lines, ["sb2se3", "method/dsc", "thermal"]);
    }

    #[test]
    fn a_summary_is_prose_and_is_not_split_into_lines() {
        let draft = Fixed("One sentence.\nAnd another.")
            .respond(&ask(Task::Summarise))
            .unwrap();
        assert!(draft.lines.is_empty());
        assert_eq!(draft.text, "One sentence.\nAnd another.");
    }

    #[test]
    fn only_the_note_and_the_tag_list_are_ever_sent() {
        // A research vault is years of unpublished work. What leaves the
        // machine should be the minimum the question needs, and visible in one
        // place — this pins that the prompt is built from `Ask` and nothing
        // else reachable.
        let asked = ask(Task::Tags);
        let prompt = user_prompt(&asked);
        assert!(prompt.contains("Sb2Se3 Cp"));
        assert!(prompt.contains("Cp fitted over 300-800 K."));
        assert!(prompt.contains("sb2se3"));
        assert!(prompt.contains("method/dsc"));

        // And for the other tasks, not even the tag list.
        let summarising = user_prompt(&ask(Task::Summarise));
        assert!(!summarising.contains("method/dsc"), "{summarising}");
    }

    #[test]
    fn the_prompt_forbids_inventing_things_and_says_why() {
        let prompt = system_prompt(&ask(Task::Summarise));
        assert!(prompt.contains("Do not invent"));
        assert!(prompt.contains("citation"));
        // Belt and braces: the prompt asks, and `drop_unknown` enforces. The
        // test above proves the enforcement works when the asking does not.
    }

    #[test]
    fn tags_are_asked_for_at_low_effort_and_reading_at_more() {
        // Latency matters in a panel beside the note being written, and
        // classifying against a list already in the prompt is not hard.
        assert_eq!(Task::Tags.effort(), "low");
        assert_eq!(Task::Summarise.effort(), "medium");
        assert_eq!(Task::Questions.effort(), "medium");
    }

    #[test]
    fn a_blank_model_falls_back_to_the_default_rather_than_failing() {
        assert_eq!(Claude::new("k".into(), "  ".into()).label(), DEFAULT_MODEL);
        assert_eq!(Claude::new("k".into(), "custom".into()).label(), "custom");
    }
}
