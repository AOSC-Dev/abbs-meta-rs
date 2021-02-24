use super::ParseErrorInfo;

pub fn get_regex_string_from_glob(glob: &str) -> Result<String, ParseErrorInfo> {
    let mut result = String::new();

    let mut in_escape = false;
    let mut in_capture_group = false;
    let mut prev_char = '\0';

    for c in glob.chars() {
        match c {
            '\\' => {
                in_escape = true;
                result.push(c);
            },
            '*' => {
                if prev_char == ']' || in_capture_group {
                    result.push('*');
                } else {
                    if !in_escape {
                        result += ".*";
                    } else {
                        result += "*";
                    }
                }
                in_escape = false;
            },
            '?' => {
                if prev_char == ']' || in_capture_group {
                    result.push(c);
                } else {
                    if !in_escape {
                        result += ".?";
                    } else {
                        result += "?";
                    }
                }
                in_escape = false;
            },
            '[' => {
                if !in_escape {
                    in_capture_group = true;
                }
                result.push(c);
                in_escape = false;
            },
            ']' => {
                if !in_escape {
                    in_capture_group = false;
                }
                result.push(c);
                in_escape = false;
            }
            '!' => {
                if prev_char == '[' {
                    result.push('^');
                } else if in_escape {
                    result.push(c);
                } else {
                    return Err(ParseErrorInfo::GlobError("Invalid token '!'.".to_string()));
                }
                in_escape = false;
            },
            _ => {
                result.push(c);
                in_escape = false;
            }
        }
        prev_char = c;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok() {
        let cases = vec![
            ("1234", "1234"),
            ("1234*", "1234.*"),
            ("[!x?*]", "[^x?*]"),
            ("[!abcd+]?", "[^abcd+]?")
        ];

        for i in cases {
            assert_eq!(get_regex_string_from_glob(i.0).unwrap(), i.1);
        }
    }

    #[test]
    fn test_bad_glob() {
        let cases = vec![
            "[!x!]",
            "_!",
        ];
        for i in cases {
            assert_eq!(get_regex_string_from_glob(i).is_ok(), false);
        }
    }
}
