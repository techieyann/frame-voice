#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Text(String),
    Submit,
    Clear,
    Cancel,
}
// Exact whole-utterance matching: fuzzy similarity is unsafe for destructive keys.
pub fn classify(text: &str) -> Output {
    let norm = text.trim().trim_end_matches(['.', '!', '?']).to_lowercase();
    let norm = norm.split_whitespace().collect::<Vec<_>>().join(" ");
    match norm.as_str() {
        "submit" | "send it" | "send" | "enter" | "go" | "send it in" => Output::Submit,
        "scratch that" | "scratch" | "clear that" | "clear it" | "clear" | "undo"
        | "delete that" | "delete it" | "delete" | "backspace that" | "backspace" => Output::Clear,
        "cancel" | "never mind" | "nevermind" | "abort" => Output::Cancel,
        _ => Output::Text(text.trim().into()),
    }
}
/// One pasteable piece. `newline` means a line break follows it (reproduce with
/// Enter), used when splitting a dictation so an app does not collapse a large
/// multi-line paste into a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub text: String,
    pub newline: bool,
}
/// Split `text` at newlines and cap each line at `max` characters (at word
/// boundaries). Concatenating the pieces with the indicated newlines reproduces
/// the input exactly.
pub fn chunks(text: &str, max: usize) -> Vec<Chunk> {
    let max = max.max(1);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let last_line = index + 1 == lines.len();
        let mut pieces: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in line.split_inclusive(' ') {
            if !current.is_empty() && current.len() + word.len() > max {
                pieces.push(std::mem::take(&mut current));
            }
            current.push_str(word);
        }
        if !current.is_empty() || pieces.is_empty() {
            pieces.push(current);
        }
        let pieces_len = pieces.len();
        for (piece_index, piece) in pieces.into_iter().enumerate() {
            out.push(Chunk {
                text: piece,
                newline: piece_index + 1 == pieces_len && !last_line,
            });
        }
    }
    out
}
pub fn strip_artifacts(text: &str) -> String {
    // Do not erase ordinary parenthesized prose, code, or paths.
    let mut text = text.to_string();
    for artifact in [
        "[BLANK_AUDIO]",
        "[blank_audio]",
        "[SILENCE]",
        "[silence]",
        "[Music]",
        "[music]",
        "(silence)",
    ] {
        text = text.replace(artifact, "");
    }
    text.trim().trim_end_matches('.').trim().into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_commands_and_preserved_content() {
        assert_eq!(classify("Submit!"), Output::Submit);
        for text in ["watch", "credit", "go now", "clear the cache", "sender"] {
            assert!(matches!(classify(text), Output::Text(_)));
        }
        assert_eq!(
            strip_artifacts("use foo(bar) and [x]."),
            "use foo(bar) and [x]"
        );
        assert_eq!(strip_artifacts("[BLANK_AUDIO]"), "");
    }
    #[test]
    fn chunks_preserve_text_and_breaks() {
        let cs = chunks("one two\nthree", 100);
        assert_eq!(
            cs,
            vec![
                Chunk {
                    text: "one two".into(),
                    newline: true
                },
                Chunk {
                    text: "three".into(),
                    newline: false
                },
            ]
        );
        // Reassembling with the newline flags reproduces the original.
        let rebuilt: String = cs
            .iter()
            .map(|c| format!("{}{}", c.text, if c.newline { "\n" } else { "" }))
            .collect();
        assert_eq!(rebuilt, "one two\nthree");
        // A long line is capped at word boundaries.
        let cs = chunks("aaaa bbbb cccc", 6);
        assert!(cs.iter().all(|c| c.text.len() <= 6));
        let rebuilt: String = cs
            .iter()
            .map(|c| format!("{}{}", c.text, if c.newline { "\n" } else { "" }))
            .collect();
        assert_eq!(rebuilt, "aaaa bbbb cccc");
    }
}
