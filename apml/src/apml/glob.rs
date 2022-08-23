use super::error::ParseErrorInfo;

pub fn get_regex_string_from_glob(glob: &str) -> Result<String, ParseErrorInfo> {
    let mut result = String::new();
    let mut idx = 0;
    let chars = glob.chars().collect::<Vec<_>>();
    let length = chars.len();
    result.reserve(length);

    while idx < length {
        match chars[idx] {
            '\\' => {
                result.push('\\');
                if idx + 1 < length {
                    idx += 1;
                    result.push(chars[idx]);
                } else {
                    return Err(ParseErrorInfo::GlobError(
                        "Incomplete escape sequence".to_string(),
                    ));
                }
            }
            '[' => {
                if idx + 1 >= length {
                    result += "\\[";
                    break;
                }
                let mut closing = idx;
                let mut found = idx;
                // find the closing bracket (greedy)
                while closing < length {
                    let capture = chars[closing];
                    if capture == ']' {
                        if closing + 1 >= length {
                            // this one must be the closing bracket
                            found = closing;
                            break;
                        }
                        found = closing;
                    // this is to ensure that the closing bracket is before another opening bracket
                    } else if capture == '[' && found > idx {
                        break;
                    } else if capture == '!'
                        && (chars[closing - 1] != '\\' && chars[closing - 1] != '[')
                    {
                        return Err(ParseErrorInfo::GlobError(
                            "Unescaped `!` symbol in a capturing group".to_string(),
                        ));
                    }
                    closing += 1;
                }
                if found <= idx {
                    // no matching closing bracket found, backtrack and interpret as non-capturing group
                    result += "\\[";
                    idx += 1;
                    continue;
                }
                result.push('[');
                idx += 1;
                if chars[idx] == '!' {
                    result.push('^');
                    idx += 1;
                }
                for c in chars[idx..found].iter() {
                    if *c == '[' || *c == ']' {
                        result.push('\\');
                    }
                    result.push(*c);
                }
                result.push(']');
                // advance the cursor to the closing brackets
                idx = found;
                // process repeating directives
                if idx + 1 >= length {
                    break;
                }
                let directive = chars[idx + 1];
                if directive == '?' || directive == '*' {
                    result.push(directive);
                    idx += 2; // +1 for ] and +1 for */?
                    continue;
                }
            }
            '*' => {
                result += ".*";
            }
            '?' => {
                result += ".?";
            }
            '!' => {
                return Err(ParseErrorInfo::GlobError(
                    "Unescaped `!` symbol".to_string(),
                ));
            }
            _ => {
                if chars[idx] == '.' || chars[idx] == '+' {
                    result.push('\\');
                }
                result.push(chars[idx]);
            }
        }
        idx += 1;
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
            ("[!abcd+]?", "[^abcd+]?"),
            ("[abcd+][!123]*", "[abcd+][^123]*"),
            ("[abcd+]?[!123]*", "[abcd+]?[^123]*"),
            ("[a][b]", "[a][b]"),
            ("[!a][!b]", "[^a][^b]"),
            ("[abc]]", "[abc\\]]"),
            ("[abc]][0[]]", "[abc\\]][0\\[\\]]"),
            ("[abc", "\\[abc"),
            ("[abc[p", "\\[abc\\[p"),
            ("[abc[", "\\[abc\\["),
        ];

        for i in cases {
            assert_eq!(get_regex_string_from_glob(i.0).unwrap(), i.1);
        }
    }

    #[test]
    fn test_bad_glob() {
        let cases = vec!["[!x!]", "_!"];
        for i in cases {
            assert!(get_regex_string_from_glob(i).is_ok());
        }
    }
}
