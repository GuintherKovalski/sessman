//! Refine prompt: a helper model rewrites the prompt in the input, or first asks clarifying
//! questions when it is ambiguous. Every question can be skipped.
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Question {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum Reply {
    Questions(Vec<Question>),
    Prompt(String),
}

/// The request to the helper model. With `answers` (one per question asked, None = skipped)
/// it must rewrite; without, it may ask up to 3 questions first.
pub fn request(text: &str, goal: Option<&str>, recent: &[String], answers: Option<&[(Question, Option<String>)]>) -> String {
    let mut p = String::from(
        "You refine prompts that a developer is about to send to Claude Code (an AI coding agent).\n\
         Rewrite the PROMPT below as a clear, well-organized prompt: keep the same intent and \
         language, add nothing the developer did not ask for, fix typos, and use short steps or \
         bullet points when it has several parts. Write it in the developer's voice.\n",
    );
    if crate::images::has_images(text) {
        p.push_str("Markers like [image:1a2b3c4d.png] are images attached to the prompt: keep each one, exactly as written, where it belongs in the text.\n");
    }
    match answers {
        None => p.push_str(
            "If the prompt is ambiguous in a way that changes what should be done, ask up to 3 \
             short clarifying questions instead, each with 2 to 4 likely answers.\n\
             Reply with JSON only, either {\"prompt\": \"<refined prompt>\"} or \
             {\"questions\": [{\"question\": \"...\", \"options\": [\"...\", \"...\"]}]}.\n",
        ),
        Some(_) => p.push_str("Do not ask questions. Reply with JSON only: {\"prompt\": \"<refined prompt>\"}.\n"),
    }
    if let Some(g) = goal {
        p.push_str(&format!("\nThe session's goal, for context:\n{g}\n"));
    }
    if !recent.is_empty() {
        p.push_str("\nThe developer's previous prompts in this session, for context:\n");
        for r in recent.iter().rev().take(3).rev() {
            let short: String = r.chars().take(400).collect();
            p.push_str(&format!("- {short}\n"));
        }
    }
    if let Some(answers) = answers {
        let answered: Vec<_> = answers.iter().filter_map(|(q, a)| a.as_ref().map(|a| (q, a))).collect();
        if !answered.is_empty() {
            p.push_str("\nThe developer answered these clarifying questions:\n");
            for (q, a) in answered {
                p.push_str(&format!("- {} → {a}\n", q.question));
            }
        }
    }
    p.push_str(&format!("\nPROMPT:\n{text}\n"));
    p
}

/// Reads the model's JSON reply, tolerating text or code fences around it.
pub fn parse(reply: &str) -> Result<Reply, String> {
    #[derive(Deserialize)]
    struct Raw {
        prompt: Option<String>,
        #[serde(default)]
        questions: Vec<Question>,
    }
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        return Err(format!("unexpected reply: {}", reply.chars().take(200).collect::<String>()));
    };
    let raw: Raw = serde_json::from_str(&reply[start..=end]).map_err(|e| format!("unexpected reply ({e})"))?;
    let questions: Vec<Question> = raw.questions.into_iter().filter(|q| !q.question.trim().is_empty()).collect();
    match raw.prompt.filter(|p| !p.trim().is_empty()) {
        Some(p) if questions.is_empty() => Ok(Reply::Prompt(p.trim().to_owned())),
        _ if !questions.is_empty() => Ok(Reply::Questions(questions)),
        _ => Err("the reply had neither a prompt nor questions".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prompt_questions_and_rejects_junk() {
        assert_eq!(parse("```json\n{\"prompt\": \" Fix the bug \"}\n```"), Ok(Reply::Prompt("Fix the bug".into())));
        let q = parse(r#"{"questions":[{"question":"Which file?","options":["a.rs","b.rs"]},{"question":" "}]}"#).unwrap();
        let Reply::Questions(qs) = q else { panic!("{q:?}") };
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].options, vec!["a.rs", "b.rs"]);
        assert!(parse("sorry, I can't").is_err());
        assert!(parse("{\"prompt\": \"\"}").is_err());
    }

    #[test]
    fn request_includes_context_and_only_answered_questions() {
        let first = request("fix it", Some("GOAL: ship"), &["p1".into()], None);
        assert!(first.contains("clarifying questions") && first.contains("GOAL: ship") && first.contains("- p1"));
        assert!(first.ends_with("PROMPT:\nfix it\n"));
        let q = |s: &str| Question { question: s.into(), options: vec![] };
        let answers = vec![(q("Which?"), Some("the login".to_owned())), (q("Skipped?"), None)];
        let second = request("fix it", None, &[], Some(&answers));
        assert!(second.contains("Do not ask questions"));
        assert!(second.contains("Which? → the login"));
        assert!(!second.contains("Skipped?"));
    }
}
