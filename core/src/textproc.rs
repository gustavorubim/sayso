//! Cleans up raw transcripts before they are delivered.

use regex::{NoExpand, Regex};

use crate::settings::Settings;

pub fn process(raw: &str, s: &Settings) -> String {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return text;
    }

    if s.remove_fillers {
        let re = Regex::new(r"(?i)\b(?:u+h+m*|u+m+|e+r+m+|h+m+)\b[,.]?\s*").unwrap();
        text = re.replace_all(&text, "").into_owned();
    }

    if s.voice_commands {
        let para = Regex::new(r"(?i)[,.]?\s*\bnew paragraph\b[,.!?]?\s*").unwrap();
        text = para.replace_all(&text, "\n\n").into_owned();
        let line = Regex::new(r"(?i)[,.]?\s*\bnew ?line\b[,.!?]?\s*").unwrap();
        text = line.replace_all(&text, "\n").into_owned();
    }

    for r in &s.replacements {
        let from = r.from.trim();
        if from.is_empty() {
            continue;
        }
        let pattern = format!(r"(?i)\b{}\b", regex::escape(from));
        if let Ok(re) = Regex::new(&pattern) {
            text = re.replace_all(&text, NoExpand(&r.to)).into_owned();
        }
    }

    let spaces = Regex::new(r"[ \t]{2,}").unwrap();
    text = spaces.replace_all(&text, " ").into_owned();
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }

    let mut out = text.to_string();
    if s.trailing_space && !out.ends_with('\n') {
        out.push(' ');
    }
    out
}

pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Replacement;

    fn base() -> Settings {
        Settings {
            trailing_space: false,
            voice_commands: false,
            remove_fillers: false,
            ..Settings::default()
        }
    }

    #[test]
    fn trims_and_adds_trailing_space() {
        let s = Settings {
            trailing_space: true,
            ..base()
        };
        assert_eq!(process("  Hello there. ", &s), "Hello there. ");
    }

    #[test]
    fn removes_fillers() {
        let s = Settings {
            remove_fillers: true,
            ..base()
        };
        assert_eq!(process("Um, I think, uh, we should go.", &s), "I think, we should go.");
        // Words that merely contain filler letters stay.
        assert_eq!(process("The umbrella hummed.", &s), "The umbrella hummed.");
    }

    #[test]
    fn voice_commands_insert_breaks() {
        let s = Settings {
            voice_commands: true,
            ..base()
        };
        assert_eq!(
            process("First point. New line. Second point.", &s),
            "First point\nSecond point."
        );
        assert_eq!(process("Intro, new paragraph, body", &s), "Intro\n\nbody");
    }

    #[test]
    fn replacements_match_whole_words_ignoring_case() {
        let s = Settings {
            replacements: vec![
                Replacement {
                    from: "my email".into(),
                    to: "me@example.com".into(),
                },
                Replacement {
                    from: "cat".into(),
                    to: "$1 dog".into(),
                },
            ],
            ..base()
        };
        assert_eq!(
            process("Send it to My Email please", &s),
            "Send it to me@example.com please"
        );
        assert_eq!(process("cat concatenate", &s), "$1 dog concatenate");
    }

    #[test]
    fn empty_stays_empty() {
        let s = Settings {
            trailing_space: true,
            remove_fillers: true,
            ..base()
        };
        assert_eq!(process("   ", &s), "");
        assert_eq!(process("Um.", &s), "");
    }

    #[test]
    fn counts_words() {
        assert_eq!(word_count("one two\nthree "), 3);
    }
}
