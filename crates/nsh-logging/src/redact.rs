//! Redaction, applied to a message before anything is written: a pure function with the
//! same output for the same input, and no I/O.

/// What a masked value becomes.
const MASK: &str = "[REDACTED]";

/// The shortest a word must be before it counts as a random-looking token.
const RANDOM_LOOKING_MIN_CHARS: usize = 20;

/// Masks what must not reach a file: the values of the listed keys (case-insensitive), as
/// `key: value`, `key = value` and the quoted JSON shapes have them; simple email
/// addresses; and long random-looking tokens such as bearer credentials. Everything else
/// passes through unchanged.
pub fn redact(input: &str, keys: &[String]) -> String {
    let keys: Vec<String> = keys
        .iter()
        .map(|key| key.trim().to_ascii_lowercase())
        .filter(|key| !key.is_empty())
        .collect();
    let words = words(input);
    let mut masked: Vec<(usize, usize)> = Vec::new();
    mask_key_values(input, &words, &keys, &mut masked);
    mask_by_shape(input, &words, &mut masked);
    // The two passes push in discovery order, not position order: sort before splicing, or
    // a shape-masked word before the last key value would be skipped as "already copied".
    masked.sort_unstable();
    rebuild(input, &masked)
}

/// The byte ranges of the words of `input`: maximal runs of characters that are neither
/// whitespace nor the punctuation that surrounds values.
fn words(input: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    for (index, character) in input.char_indices() {
        if is_word_char(character) {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(begin) = start.take() {
            ranges.push((begin, index));
        }
    }
    if let Some(begin) = start {
        ranges.push((begin, input.len()));
    }
    ranges
}

/// `@` and `.` stay inside a word, so an email address is one word.
fn is_word_char(character: char) -> bool {
    !character.is_whitespace()
        && !matches!(
            character,
            ',' | ';' | ':' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '='
        )
}

/// Masks the value word after each listed key.
fn mask_key_values(
    input: &str,
    words: &[(usize, usize)],
    keys: &[String],
    masked: &mut Vec<(usize, usize)>,
) {
    for (position, &(start, end)) in words.iter().enumerate() {
        let key = input[start..end].to_ascii_lowercase();
        if !keys.contains(&key) {
            continue;
        }
        let Some(&(value_start, value_end)) = words.get(position + 1) else {
            continue;
        };
        // Only a colon or an equals (spaces and quotes aside) makes the next word a value.
        let separator: String = input[end..value_start]
            .chars()
            .filter(|&character| !matches!(character, ' ' | '"' | '\''))
            .collect();
        let mut characters = separator.chars();
        let introduces = matches!(characters.next(), Some(':' | '='));
        if !introduces || characters.any(|character| character != ':' && character != '=') {
            continue;
        }
        if !masked.iter().any(|&(from, _)| from == value_start) {
            masked.push((value_start, value_end));
        }
    }
}

/// Masks what gives itself away by shape: an email address, or a long token mixing letters
/// and digits as session ids and credentials look.
fn mask_by_shape(input: &str, words: &[(usize, usize)], masked: &mut Vec<(usize, usize)>) {
    for &(start, end) in words {
        if masked.iter().any(|&(from, until)| from <= start && start < until) {
            continue;
        }
        let word = &input[start..end];
        if is_email(word) || is_random_looking(word) {
            masked.push((start, end));
        }
    }
}

/// A simple email address: one `@`, a dotted domain, and a TLD of two letters or more.
fn is_email(word: &str) -> bool {
    let Some((name, domain)) = word.split_once('@') else {
        return false;
    };
    if name.is_empty() || domain.contains('@') {
        return false;
    }
    let Some((_, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic)
}

fn is_random_looking(word: &str) -> bool {
    word.chars().count() >= RANDOM_LOOKING_MIN_CHARS
        && word.chars().any(char::is_ascii_digit)
        && word.chars().any(char::is_alphabetic)
}

/// Splices `MASK` over the masked ranges, sorted by position, keeping every other byte of
/// the input.
fn rebuild(input: &str, masked: &[(usize, usize)]) -> String {
    let mut output = String::with_capacity(input.len());
    let mut copied = 0;
    for &(start, end) in masked {
        if start < copied || start >= end {
            continue;
        }
        output.push_str(&input[copied..start]);
        output.push_str(MASK);
        copied = end;
    }
    output.push_str(&input[copied..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn key_values_are_masked_in_every_shape() {
        let keys = keys(&["token", "password"]);
        assert_eq!(redact("token=abc123", &keys), "token=[REDACTED]");
        assert_eq!(redact("TOKEN: hunter2", &keys), "TOKEN: [REDACTED]");
        assert_eq!(
            redact(r#""password": "hunter2", "safe": "kept""#, &keys),
            r#""password": "[REDACTED]", "safe": "kept""#
        );
        assert_eq!(redact("token:", &keys), "token:");
        assert_eq!(redact("nothing here", &keys), "nothing here");
    }

    #[test]
    fn emails_and_long_tokens_are_masked_without_a_key() {
        assert_eq!(redact("mail me@example.org", &[]), "mail [REDACTED]");
        assert_eq!(redact("id 3f2A9dQwLm4Zx7Cv1Bn5Kj", &[]), "id [REDACTED]");
        assert_eq!(redact("a@b", &[]), "a@b");
        assert_eq!(redact("plain words stay", &[]), "plain words stay");
    }

    #[test]
    fn a_masked_word_before_a_key_value_is_masked_too() {
        // The shape pass runs after the key pass, so its ranges arrive out of position
        // order; the splice must still mask everything.
        let keys = keys(&["token"]);
        assert_eq!(
            redact("mail me@example.org token=abc", &keys),
            "mail [REDACTED] token=[REDACTED]"
        );
        assert_eq!(
            redact("me@example.org token=abc id 3f2A9dQwLm4Zx7Cv1Bn5Kj", &keys),
            "[REDACTED] token=[REDACTED] id [REDACTED]"
        );
    }
}
