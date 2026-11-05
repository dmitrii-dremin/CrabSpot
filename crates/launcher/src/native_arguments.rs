//! OS transport only: language values remain exact UTF-16.
use crabspot_launcher::{ApplicationStringLimits, ArgumentError, ArgumentResource};
use std::ffi::{OsStr, OsString};
fn over(
    index: usize,
    total: u64,
    allowance: u64,
    limits: ApplicationStringLimits,
) -> ArgumentError {
    let (resource, limit, old) = if allowance < limits.max_units_per_argument {
        (
            ArgumentResource::ArgumentUnits,
            limits.max_argument_units,
            total,
        )
    } else {
        (
            ArgumentResource::UnitsPerArgument,
            limits.max_units_per_argument,
            0,
        )
    };
    match old.checked_add(allowance).and_then(|n| n.checked_add(1)) {
        Some(required) => ArgumentError::Limit {
            index: Some(index),
            resource,
            limit,
            required,
        },
        None => ArgumentError::Overflow {
            index: Some(index),
            resource,
        },
    }
}
fn units(
    value: &OsStr,
    index: usize,
    total: u64,
    limits: ApplicationStringLimits,
) -> Result<Vec<u16>, ArgumentError> {
    let allowance = limits
        .max_units_per_argument
        .min(limits.max_argument_units - total);
    #[cfg(windows)]
    let iterator = {
        use std::os::windows::ffi::OsStrExt;
        value.encode_wide()
    };
    #[cfg(unix)]
    let iterator = {
        use std::os::unix::ffi::OsStrExt;
        if value.as_bytes().len() as u64 > allowance.saturating_mul(3) {
            return Err(over(index, total, allowance, limits));
        }
        value
            .to_str()
            .ok_or(ArgumentError::Encoding { index })?
            .encode_utf16()
    };
    let mut n = 0u64;
    for _ in iterator {
        n = n.checked_add(1).ok_or(ArgumentError::Overflow {
            index: Some(index),
            resource: ArgumentResource::UnitsPerArgument,
        })?;
        if n > allowance {
            return Err(over(index, total, allowance, limits));
        }
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(
            usize::try_from(n)
                .map_err(|_| ArgumentError::AllocationFailure { index: Some(index) })?,
        )
        .map_err(|_| ArgumentError::AllocationFailure { index: Some(index) })?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        result.extend(value.encode_wide());
    }
    #[cfg(unix)]
    result.extend(
        value
            .to_str()
            .ok_or(ArgumentError::Encoding { index })?
            .encode_utf16(),
    );
    Ok(result)
}
pub(crate) fn decode(
    arguments: impl ExactSizeIterator<Item = OsString>,
    limits: ApplicationStringLimits,
) -> Result<Vec<Vec<u16>>, ArgumentError> {
    let count = arguments.len() as u64;
    let limit = limits.max_arguments.min(i32::MAX as u64);
    if count > limit {
        return Err(ArgumentError::Limit {
            index: None,
            resource: ArgumentResource::Arguments,
            limit,
            required: count,
        });
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(arguments.len())
        .map_err(|_| ArgumentError::AllocationFailure { index: None })?;
    let mut total = 0;
    for (index, argument) in arguments.enumerate() {
        let value = units(&argument, index, total, limits)?;
        total += value.len() as u64; // bounded by the checked remaining allowance
        result.push(value);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_units_and_ordinary_spellings() {
        let values = ["", "--", " left ", "x\0y", "\u{1f980}"];
        let decoded = decode(values.into_iter().map(OsString::from), Default::default()).unwrap();
        assert_eq!(
            decoded,
            values.map(|s| s.encode_utf16().collect::<Vec<_>>())
        );
    }
    #[test]
    fn count_first_and_width_before_copy() {
        let limits = ApplicationStringLimits {
            max_arguments: 0,
            ..Default::default()
        };
        assert!(matches!(
            decode([OsString::from("x")].into_iter(), limits),
            Err(ArgumentError::Limit {
                index: None,
                resource: ArgumentResource::Arguments,
                required: 1,
                ..
            })
        ));
        let limits = ApplicationStringLimits {
            max_units_per_argument: 1,
            ..Default::default()
        };
        assert!(matches!(
            decode([OsString::from("abc")].into_iter(), limits),
            Err(ArgumentError::Limit {
                index: Some(0),
                resource: ArgumentResource::UnitsPerArgument,
                required: 2,
                ..
            })
        ));
        let limits = ApplicationStringLimits {
            max_argument_units: 2,
            ..Default::default()
        };
        assert!(matches!(
            decode(
                [OsString::from("x"), OsString::from("yz")].into_iter(),
                limits
            ),
            Err(ArgumentError::Limit {
                index: Some(1),
                resource: ArgumentResource::ArgumentUnits,
                required: 3,
                ..
            })
        ));
    }
    #[cfg(windows)]
    #[test]
    fn windows_preserves_unpaired_surrogates() {
        use std::os::windows::ffi::OsStringExt;
        let raw = [0xd800, 0, 0xdc00, 0x20ac];
        assert_eq!(
            decode([OsString::from_wide(&raw)].into_iter(), Default::default()).unwrap(),
            [raw]
        );
    }
    #[cfg(unix)]
    #[test]
    fn unix_invalid_encoding_and_bounded_oversize() {
        use std::os::unix::ffi::OsStringExt;
        let invalid = OsString::from_vec(vec![0xff]);
        assert_eq!(
            decode([invalid.clone()].into_iter(), Default::default()),
            Err(ArgumentError::Encoding { index: 0 })
        );
        let limits = ApplicationStringLimits {
            max_units_per_argument: 0,
            ..Default::default()
        };
        assert!(matches!(
            decode([invalid].into_iter(), limits),
            Err(ArgumentError::Limit { required: 1, .. })
        ));
    }
}
