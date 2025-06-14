use std::borrow::Cow;

use thiserror::Error;

/// Escape `"`, `\` or ` ` characters
pub fn escape(s: &str) -> Cow<'_, str> {
    let mut quote = false;
    let mut needs_per_char_quote = false;

    for c in s.chars() {
        quote |= match c {
            '"' | '\\' => {
                needs_per_char_quote = true;
                quote = true;
                break;
            }
            ' ' => true,
            _ => false,
        };
    }

    if !quote {
        return Cow::from(s);
    }
    if !needs_per_char_quote {
        return format!("\"{s}\"").into();
    }

    let mut output = String::with_capacity(s.len());
    output.push('"');
    for c in s.chars() {
        if c == '"' {
            output += "\\\"";
        } else if c == '\\' {
            output += "\\\\";
        } else {
            output.push(c);
        }
    }
    output.push('"');
    output.into()
}

/// Error thrown by [`unescape`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnescapeError {
    #[error("invalid escape {escape} at {index} in {string}")]
    InvalidEscape {
        escape: String,
        index: usize,
        string: String,
    },
}

/// Parse an escaped quoted string, e.g. one produced by [`escape`].
pub fn unescape(s: &str) -> Result<String, UnescapeError> {
    let mut in_double_quote = false;

    let mut chars = s.chars().enumerate();

    let mut res = String::with_capacity(s.len());

    while let Some((index, c)) = chars.next() {
        // when quoted, check for escapes
        if in_double_quote {
            if c == '"' {
                in_double_quote = false;
                continue;
            }

            if c == '\\' {
                match chars.next() {
                    None => {
                        return Err(UnescapeError::InvalidEscape {
                            escape: format!("{c}"),
                            index,
                            string: String::from(s),
                        });
                    }
                    Some((index, c2)) => {
                        res.push(match c2 {
                            '\\' => '\\',
                            '"' => '"',
                            ' ' => ' ',
                            _ => {
                                return Err(UnescapeError::InvalidEscape {
                                    escape: format!("{c}{c2}"),
                                    index,
                                    string: String::from(s),
                                });
                            }
                        });
                        continue;
                    }
                };
            }
        } else if c == '"' {
            in_double_quote = true;
            continue;
        }

        res.push(c);
    }

    Ok(res)
}
