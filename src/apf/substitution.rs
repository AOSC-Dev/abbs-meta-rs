use super::ParseErrorInfo;

use regex::Regex;

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

    if begin > origin.len() {
        return Err(ParseErrorInfo::SubstitutionError(
            "Begin is than length.".to_string(),
        ));
    }

    match length {
        Some(l) => {
            if begin + l > origin.len() {
                return Err(ParseErrorInfo::SubstitutionError(
                    "End of substring bigger than length.".to_string(),
                ));
            }

            Ok(origin[begin..begin + l].to_string())
        }
        None => Ok(origin[begin..].to_string()),
    }
}

fn parse_number(s: &str) -> Result<usize, ParseErrorInfo> {
    let res: usize = match s.parse() {
        Ok(r) => r,
        Err(_e) => {
            return Err(ParseErrorInfo::InvalidSyntax(
                "Bad number in subsitution.".to_string(),
            ));
        }
    };

    Ok(res)
}

pub fn get_replace(origin: &str, command: &str, all: bool) -> Result<String, ParseErrorInfo> {
    let (from, to) = match command.chars().filter(|c| c == &'/').count() {
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

    let re = Regex::new(&from)?;
    let result = match all {
        true => re.replace_all(origin, to.as_str()),
        false => re.replace(origin, to.as_str()),
    };

    Ok(result.to_string())
}
