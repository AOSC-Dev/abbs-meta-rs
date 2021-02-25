use super::{glob::get_regex_string_from_glob, ParseErrorInfo};

use regex::Regex;
use std::cmp;

/// Substring in bash subsitution.
/// i.e: ${variable:BEGIN:LENGTH}
pub fn get_substring(origin: &str, command: &str) -> Result<String, ParseErrorInfo> {
    let (begin, length) = match command.chars().filter(|c| c == &':').count() {
        0 => (parse_number(command)?, None),
        1 => {
            let commands: Vec<&str> = command.split(":").collect();
            (parse_number(commands[0])?, Some(parse_number(commands[1])?))
        }
        _ => {
            return Err(ParseErrorInfo::InvalidSyntax(
                "Bad substring command.".to_string(),
            ));
        }
    };

    let real_begin = if begin >= 0 {
        cmp::min(origin.len(), begin as usize)
    } else {
        cmp::max(origin.len() - (begin.abs() as usize), 0)
    };

    match length {
        Some(length) => {
            if length >= 0 {
                let real_end = cmp::min(origin.len(), real_begin + length as usize);
                Ok(origin[real_begin..real_end].to_string())
            } else {
                let max_len = origin.len() - real_begin;
                let real_length = cmp::max(0, max_len - length.abs() as usize);
                let real_end = cmp::min(origin.len(), real_begin + real_length);
                Ok(origin[real_begin..real_end].to_string())
            }
        }
        None => Ok(origin[real_begin..].to_string()),
    }
}

fn parse_number(s: &str) -> Result<isize, ParseErrorInfo> {
    // Bash magic!
    if s.len() == 0 {
        return Ok(0);
    }
    let left_bracket_count = s.chars().filter(|c| c == &'(').count();
    let right_bracket_count = s.chars().filter(|c| c == &')').count();

    let mut s = s.to_string();
    if left_bracket_count == 1 && right_bracket_count == 1 {
        s = s.chars().filter(|c| c != &'(' && c != &')').collect();
    } else if left_bracket_count != 0 || right_bracket_count != 0 {
        return Err(ParseErrorInfo::InvalidSyntax(
            "Bad parentheses in number.".to_string(),
        ));
    }

    let res: isize = match s.parse() {
        Ok(r) => r,
        Err(_e) => {
            return Err(ParseErrorInfo::InvalidSyntax(
                "Bad number in substitution.".to_string(),
            ));
        }
    };

    Ok(res)
}

fn get_chars_without_escape(c: &char, s: &str) -> usize {
    let mut result = 0;
    let mut prev_char = '\0';

    for i in s.chars() {
        if prev_char != '\\' && &i == c {
            result += 1;
        }
        prev_char = i;
    }

    result
}

pub fn get_replace(origin: &str, command: &str, all: bool) -> Result<String, ParseErrorInfo> {
    let (from, to) = match get_chars_without_escape(&'/', command) {
        1 => {
            let commands: Vec<&str> = command.split("/").collect();
            (commands[0].to_string(), commands[1].to_string())
        }
        _ => {
            return Err(ParseErrorInfo::InvalidSyntax(
                "Invalid replace command.".to_string(),
            ));
        }
    };

    let re = Regex::new(&get_regex_string_from_glob(&from)?)?;
    let result = match all {
        true => re.replace_all(origin, to.as_str()),
        false => re.replace(origin, to.as_str()),
    };

    Ok(result.to_string())
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
            assert_eq!(get_substring(origin, c).is_ok(), false);
        }
    }
}
