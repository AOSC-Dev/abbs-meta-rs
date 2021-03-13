use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum FailArch {
    Include(Vec<String>),
    Exclude(Vec<String>),
    Empty,
}

impl FailArch {
    pub fn from(s: &str) -> Result<Self, ()> {
        let chars: Vec<char> = s.chars().collect();
        if !s.is_empty() && chars[0] == '!' {
            let archs = get_arch_from_set(&s[1..])?;
            Ok(FailArch::Exclude(archs))
        } else {
            let archs = get_arch_from_set(s)?;
            Ok(FailArch::Include(archs))
        }
    }
}

/// Example: "(amd64|arm64)"
fn get_arch_from_set(s: &str) -> Result<Vec<String>, ()> {
    let chars: Vec<char> = s.chars().collect();

    if chars[0] == '(' && chars[chars.len() - 1] == ')' {
        let archs_str: String = chars[1..chars.len() - 1].iter().collect();
        // Split them into individual arches
        let archs: Vec<String> = archs_str.split('|').map(|s| s.to_string()).collect();
        Ok(archs)
    } else if chars
        .into_iter()
        .filter(|c| c == &'|' || c == &'(' || c == &')')
        .count()
        == 0
    {
        let arch = s.to_string();
        Ok(vec![arch])
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_parsing() {
        let ok_cases = vec![
            (
                "(ppc64|powerpc)",
                vec!["ppc64".to_string(), "powerpc".to_string()],
            ),
            ("(ppc64)", vec!["ppc64".to_string()]),
            ("ppc64", vec!["ppc64".to_string()]),
        ];

        let bad_cases = vec!["ppc64|amd64", "ppc64|(amd64|arm64)"];

        for (case, res) in ok_cases {
            assert_eq!(get_arch_from_set(&case), Ok(res));
        }

        for case in bad_cases {
            assert_eq!(get_arch_from_set(&case), Err(()));
        }
    }
}
