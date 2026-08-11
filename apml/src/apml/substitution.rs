use super::{ast::ReplaceAnchor, error::ParseErrorInfo, glob::get_regex_string_from_glob};

use regex::Regex;
use std::borrow::Cow;
use std::cmp;

/// Substring in bash subsitution.
/// i.e: ${variable:BEGIN:LENGTH}
pub fn get_substring(origin: &str, command: &str) -> Result<String, ParseErrorInfo> {
    let (begin, length) = match command.chars().filter(|c| c == &':').count() {
        0 => (parse_number(command)?, None),
        1 => {
            let commands: Vec<&str> = command.split(':').collect();
            (parse_number(commands[0])?, Some(parse_number(commands[1])?))
        }
        _ => {
            return Err(ParseErrorInfo::SubstitutionError(
                "Bad substring command.".to_string(),
                command.to_string(),
            ));
        }
    };

    let real_begin = if begin >= 0 {
        cmp::min(origin.len(), begin as usize)
    } else {
        cmp::max(origin.len() - begin.unsigned_abs(), 0)
    };

    match length {
        Some(length) => {
            if length >= 0 {
                let real_end = cmp::min(origin.len(), real_begin + length as usize);
                Ok(origin[real_begin..real_end].to_string())
            } else {
                let max_len = origin.len() - real_begin;
                let real_length = cmp::max(0, max_len - length.unsigned_abs());
                let real_end = cmp::min(origin.len(), real_begin + real_length);
                Ok(origin[real_begin..real_end].to_string())
            }
        }
        None => Ok(origin[real_begin..].to_string()),
    }
}

fn parse_number(s: &str) -> Result<isize, ParseErrorInfo> {
    // Bash magic!
    if s.is_empty() {
        return Ok(0);
    }
    let left_bracket_count = s.chars().filter(|c| c == &'(').count();
    let right_bracket_count = s.chars().filter(|c| c == &')').count();

    // Strip a balanced pair of parentheses without allocating in the common
    // no-parenthesis case.
    let num_str: Cow<'_, str> = if left_bracket_count == 1 && right_bracket_count == 1 {
        Cow::Owned(s.chars().filter(|c| c != &'(' && c != &')').collect())
    } else if left_bracket_count != 0 || right_bracket_count != 0 {
        return Err(ParseErrorInfo::InvalidSyntax(
            "Bad parentheses in number.".to_string(),
        ));
    } else {
        Cow::Borrowed(s)
    };

    num_str
        .parse()
        .map_err(|_| ParseErrorInfo::InvalidSyntax("Bad number in substitution.".to_string()))
}

/// Replace occurrences of `pattern` (a glob) in `origin` with `replacement`.
///
/// `all` selects `${name/pat/rep}` (once) vs `${name//pat/rep}` (all).
/// `anchor` implements the `${name/#pat/rep}` (prefix) and `${name/%pat/rep}`
/// (suffix) forms.
pub fn get_replace(
    origin: &str,
    pattern: &str,
    replacement: &str,
    all: bool,
    anchor: ReplaceAnchor,
) -> Result<String, ParseErrorInfo> {
    let mut re = get_regex_string_from_glob(pattern)?;
    match anchor {
        ReplaceAnchor::Any => {}
        ReplaceAnchor::Prefix => re = format!("^(?:{})", re),
        ReplaceAnchor::Suffix => re = format!("(?:{})$", re),
    }
    let re = Regex::new(&re)?;
    let result = if all {
        re.replace_all(origin, replacement)
    } else {
        re.replace(origin, replacement)
    };
    // `into_owned` moves when the replacement produced an owned string.
    Ok(result.into_owned())
}

/// Returns the string with prefix or suffix removed according to the given pattern.
///
/// mode: true: prefix removal, false: suffix removal
pub fn get_trim_prefix(
    origin: &str,
    pattern: &str,
    mode: bool,
    greedy: bool,
) -> Result<String, ParseErrorInfo> {
    let mut regex_pattern = get_regex_string_from_glob(pattern)?;
    let trim_pattern;
    if mode {
        if !greedy {
            regex_pattern = regex_pattern.replace(".*", ".*?");
        }
        trim_pattern = format!("^(?:{})?(.*)$", regex_pattern);
    } else {
        trim_pattern = format!(
            "^(.*{})(?:{})$",
            if greedy { "?" } else { "" },
            regex_pattern
        );
    }
    let regex = Regex::new(&trim_pattern)?;
    if let Some(result) = regex.captures(origin) {
        if let Some(capture) = result.get(1) {
            return Ok(capture.as_str().to_string());
        }
    } else if !mode {
        return Ok(origin.to_string());
    }

    Err(ParseErrorInfo::GlobError(
        "Match failed with converted pattern.".to_string(),
    ))
}

#[inline]
fn lowercase_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
    }
}

#[inline]
fn uppercase_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn get_lower_case(
    origin: &str,
    pattern: Option<&str>,
    all: bool,
) -> Result<String, ParseErrorInfo> {
    if pattern.is_none() {
        if all {
            return Ok(origin.to_lowercase());
        } else {
            return Ok(lowercase_first_letter(origin));
        }
    }

    let re = Regex::new(&get_regex_string_from_glob(pattern.unwrap())?)?;
    if all {
        Ok(re
            .replace_all(origin, |caps: &regex::Captures| caps[0].to_lowercase())
            .into_owned())
    } else {
        Ok(re
            .replace(origin, |caps: &regex::Captures| caps[0].to_lowercase())
            .into_owned())
    }
}

pub fn get_upper_case(
    origin: &str,
    pattern: Option<&str>,
    all: bool,
) -> Result<String, ParseErrorInfo> {
    if pattern.is_none() {
        if all {
            return Ok(origin.to_uppercase());
        } else {
            return Ok(uppercase_first_letter(origin));
        }
    }

    let re = Regex::new(&get_regex_string_from_glob(pattern.unwrap())?)?;
    if all {
        Ok(re
            .replace_all(origin, |caps: &regex::Captures| caps[0].to_uppercase())
            .into_owned())
    } else {
        Ok(re
            .replace(origin, |caps: &regex::Captures| caps[0].to_uppercase())
            .into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substring() {
        // Bash magic!
        let origin = "1234567890";
        let ok_cases = vec![
            ("0:1", "1"),
            ("(0):1", "1"),
            ("(-1):(1)", "0"),
            (":7", "1234567"),
            ("0", "1234567890"),
            ("(-1):(-1)", ""),
            ("(0):(-1)", "123456789"),
        ];
        let err_cases = vec!["(:1", "(:1)"];

        for c in ok_cases {
            assert_eq!(get_substring(origin, c.0).unwrap(), c.1);
        }
        for c in err_cases {
            assert!(get_substring(origin, c).is_err());
        }
    }
}
