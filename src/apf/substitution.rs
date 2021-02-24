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

    if begin >= 0 {
        let real_begin = begin as usize;
        let mut real_end = origin.len();
        if let Some(len) = length {
            if len > 0 {
                real_end = real_begin + (len as usize);
            } else {
                real_end = origin.len() - cmp::min(origin.len(), len.abs() as usize);
            }
        }
        // Trim both ends
        if real_begin > origin.len() || real_end <= real_begin {
            return Ok("".to_string());
        }

        Ok(origin[real_begin..real_end].to_string())
    } else {
        Ok(origin.to_string())
    }
}

fn parse_number(s: &str) -> Result<isize, ParseErrorInfo> {
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
        let origin = "123456789";
        let cases = vec![
            ("1:3", "234"),
            ("1:-3", "23456"),
            ("-1:3", "123456789"),
            ("-1:-3", "123456789"),
            ("0:-3", "123456"),
            ("-3:0", "123456789"),
            ("3", "456789"),
            ("-3", "123456789"),
        ];

        for c in cases {
            assert_eq!(get_substring(origin, c.0).unwrap(), c.1);
        }
    }
}
