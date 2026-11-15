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
    if text.trim().is_empty() {
        return String::new();
    }
    // Drop stray spaces at the ends, but keep a line break the speaker asked for: ending a
    // dictation with "new line" should leave the cursor on a new line.
    let text = text.trim_matches([' ', '\t']);

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

    #[test]
    fn everything_together_in_a_fixed_order() {
        let s = Settings {
            remove_fillers: true,
            voice_commands: true,
            trailing_space: true,
            replacements: vec![Replacement {
                from: "acme".into(),
                to: "ACME Corp".into(),
            }],
            ..Settings::default()
        };
        assert_eq!(
            process("  Um, tell acme we agree. New paragraph. Uh, send it today.  ", &s),
            "tell ACME Corp we agree\n\nsend it today. "
        );
    }

    #[test]
    fn a_trailing_space_is_not_added_after_a_line_break() {
        let s = Settings {
            voice_commands: true,
            trailing_space: true,
            ..base()
        };
        // Ending with "new line" leaves the cursor on a new line, with no space after it.
        assert_eq!(process("first line new line", &s), "first line\n");
        assert_eq!(process("done. New paragraph.", &s), "done\n\n");
        // Starting with one works too.
        assert_eq!(process("new line second point", &s), "\nsecond point ");
    }

    #[test]
    fn replacements_handle_punctuation_and_other_scripts() {
        let s = Settings {
            replacements: vec![
                Replacement {
                    from: "c plus plus".into(),
                    to: "C++".into(),
                },
                Replacement {
                    from: "são paulo".into(),
                    to: "São Paulo".into(),
                },
                Replacement {
                    from: "  ".into(),
                    to: "ignored".into(),
                },
                Replacement {
                    from: "bye".into(),
                    to: String::new(),
                },
            ],
            ..base()
        };
        assert_eq!(
            process("I write C Plus Plus in são paulo, bye", &s),
            "I write C++ in São Paulo,"
        );
    }

    #[test]
    fn text_without_anything_to_change_is_left_alone() {
        let s = base();
        for text in ["Hello, world.", "Ünïcödé stays.", "数字 123", "a single\ttab stays"] {
            assert_eq!(process(text, &s), text);
        }
    }

    #[test]
    fn voice_commands_can_be_switched_off() {
        let s = base();
        assert_eq!(process("add a new line here", &s), "add a new line here");
    }

    #[test]
    fn spaces_and_tabs_are_collapsed_but_line_breaks_survive() {
        let s = Settings {
            voice_commands: true,
            ..base()
        };
        assert_eq!(process("one   two\t\tthree new line four", &s), "one two three\nfour");
    }
}
