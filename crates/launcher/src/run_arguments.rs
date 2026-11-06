//! Bounded pre-entry CLI grammar; native guest tokens are never reinterpreted.
use std::{ffi::OsString, path::PathBuf};
#[derive(Debug)]
pub(super) struct RunArguments {
    pub platform: PathBuf,
    pub classes: PathBuf,
    pub entry: OsString,
    pub field_lookup_steps: Option<usize>,
}
#[derive(Debug)]
pub(super) struct Usage;
fn quota(value: &std::ffi::OsStr) -> Result<usize, Usage> {
    let bytes = value.as_encoded_bytes();
    if bytes.is_empty() || bytes.len() > 20 {
        return Err(Usage);
    }
    let mut result = 0usize;
    for &digit in bytes {
        if !digit.is_ascii_digit() {
            return Err(Usage);
        }
        result = result
            .checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(digit - b'0')))
            .ok_or(Usage)?;
    }
    Ok(result)
}
pub(super) fn parse(
    arguments: &mut impl ExactSizeIterator<Item = OsString>,
) -> Result<RunArguments, Usage> {
    let mut platform = None;
    let mut classes = None;
    let mut field_lookup_steps = None;
    let mut pairs = 0;
    let entry = loop {
        let token = arguments.next().ok_or(Usage)?;
        if token == "--platform" || token == "--classes" || token == "--field-lookup-steps" {
            if pairs == 3 {
                return Err(Usage);
            }
            pairs += 1;
            let value = arguments.next().ok_or(Usage)?;
            if token == "--platform" && platform.is_none() {
                platform = Some(PathBuf::from(value));
            } else if token == "--classes" && classes.is_none() {
                classes = Some(PathBuf::from(value));
            } else if token == "--field-lookup-steps" && field_lookup_steps.is_none() {
                field_lookup_steps = Some(quota(&value)?);
            } else {
                return Err(Usage);
            }
        } else if token.as_encoded_bytes().starts_with(b"--") {
            return Err(Usage);
        } else {
            break token;
        }
    };
    let platform = platform.ok_or(Usage)?;
    let classes = classes.ok_or(Usage)?;
    if let Some(separator) = arguments.next() {
        if separator != "--" {
            return Err(Usage);
        }
    }
    Ok(RunArguments {
        platform,
        classes,
        entry,
        field_lookup_steps,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> std::vec::IntoIter<OsString> {
        values
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }
    #[test]
    fn all_pair_orders_and_legacy_required_orders_preserve_the_same_options() {
        let groups = [
            ["--platform", "p"],
            ["--classes", "c"],
            ["--field-lookup-steps", "16777216"],
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut tokens = order
                .into_iter()
                .flat_map(|i| groups[i])
                .collect::<Vec<_>>();
            tokens.push("Main");
            let p = parse(&mut args(&tokens)).unwrap();
            assert_eq!(p.platform, PathBuf::from("p"));
            assert_eq!(p.classes, PathBuf::from("c"));
            assert_eq!(p.field_lookup_steps, Some(16777216));
        }
        for order in [
            ["--platform", "p", "--classes", "c", "Main"],
            ["--classes", "c", "--platform", "p", "Main"],
        ] {
            assert_eq!(parse(&mut args(&order)).unwrap().field_lookup_steps, None);
        }
    }
    #[test]
    fn checked_ascii_zeros_max_overflow_and_length_bound() {
        for value in ["0", "00000000000000000000"] {
            assert_eq!(quota(std::ffi::OsStr::new(value)).unwrap(), 0);
        }
        assert_eq!(
            quota(std::ffi::OsStr::new(&usize::MAX.to_string())).unwrap(),
            usize::MAX
        );
        for value in [
            "",
            "-1",
            "+1",
            " 1",
            "1 ",
            "1_0",
            "0x1",
            "\u{ff11}",
            "\u{0661}",
            "000000000000000000000",
            "18446744073709551616",
        ] {
            assert!(quota(std::ffi::OsStr::new(value)).is_err(), "{value:?}");
        }
    }
    #[test]
    fn malformed_grammar_is_rejected_without_directory_io() {
        for values in [
            vec![],
            vec!["--"],
            vec!["--platform"],
            vec!["--platform", "p", "--classes", "c"],
            vec!["--platform", "p", "--classes", "c", "--field-lookup-steps"],
            vec![
                "--platform",
                "p",
                "--classes",
                "c",
                "--field-lookup-steps",
                "",
            ],
            vec!["--platform", "p", "--platform", "q", "Main"],
            vec![
                "--platform",
                "p",
                "--classes",
                "c",
                "--unknown",
                "1",
                "Main",
            ],
            vec![
                "--platform",
                "p",
                "--classes",
                "c",
                "Main",
                "--field-lookup-steps",
                "1",
            ],
            vec![
                "--platform",
                "p",
                "--classes",
                "c",
                "--field-lookup-steps",
                "0",
                "--field-lookup-steps",
                "1",
                "Main",
            ],
        ] {
            assert!(parse(&mut args(&values)).is_err(), "{values:?}");
        }
    }
    #[test]
    fn guest_option_tokens_after_separator_are_exact_native_values() {
        let mut a = args(&[
            "--platform",
            "p",
            "--classes",
            "c",
            "Main",
            "--",
            "--field-lookup-steps",
            "not-a-number",
            "--classes",
        ]);
        let p = parse(&mut a).unwrap();
        assert_eq!(p.entry, OsString::from("Main"));
        assert_eq!(
            a.collect::<Vec<_>>(),
            ["--field-lookup-steps", "not-a-number", "--classes"].map(OsString::from)
        );
    }
    #[cfg(windows)]
    #[test]
    fn invalid_native_quota_is_usage_but_actual_entry_stays_native() {
        use std::os::windows::ffi::OsStringExt;
        let invalid = OsString::from_wide(&[0xd800]);
        assert!(quota(&invalid).is_err());
        let mut a = vec![
            "--platform".into(),
            "p".into(),
            "--classes".into(),
            "c".into(),
            invalid,
        ]
        .into_iter();
        assert!(parse(&mut a).unwrap().entry.into_string().is_err());
    }
    #[cfg(unix)]
    #[test]
    fn invalid_native_quota_is_usage_but_actual_entry_stays_native() {
        use std::os::unix::ffi::OsStringExt;
        let invalid = OsString::from_vec(vec![0xff]);
        assert!(quota(&invalid).is_err());
        let mut a = vec![
            "--platform".into(),
            "p".into(),
            "--classes".into(),
            "c".into(),
            invalid,
        ]
        .into_iter();
        assert!(parse(&mut a).unwrap().entry.into_string().is_err());
    }
}
