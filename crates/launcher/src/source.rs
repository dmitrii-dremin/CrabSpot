use crate::{
    ApplicationDirectories, ApplicationErrorKind as K, ApplicationResource as R,
    ApplicationStage as S, ApplicationUsage, InputRole,
};
use crate::{error::Result, meter::Meter};
use std::{
    cmp::Ordering,
    fs::{self, File, Metadata},
    io::Read,
    path::{Component, Path, PathBuf},
};
pub(crate) struct Leaf {
    pub(crate) path: PathBuf,
    pub(crate) relative: String,
    pub(crate) units: Vec<u16>,
    pub(crate) class_file: bool,
}
fn link(m: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}
pub(crate) fn component(s: &str) -> bool {
    if s.is_empty()
        || matches!(s, "." | "..")
        || s.ends_with([' ', '.'])
        || s.chars()
            .any(|c| c <= '\u{1f}' || c == '\u{7f}' || "<>:\"|*?/\\".contains(c))
    {
        return false;
    }
    let stem = s.split('.').next().unwrap_or("");
    !["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|v| stem.eq_ignore_ascii_case(v))
        && !(stem.len() == 4
            && (stem.as_bytes()[..3].eq_ignore_ascii_case(b"COM")
                || stem.as_bytes()[..3].eq_ignore_ascii_case(b"LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
fn root(path: &Path, m: &mut Meter, u: ApplicationUsage) -> Result<PathBuf> {
    m.work(path.as_os_str().len() as u64, S::Input, u)?;
    let spelling = path
        .to_str()
        .ok_or_else(|| m.error(S::Input, K::InvalidPath("unsupported root encoding"), u))?;
    m.utf(spelling.encode_utf16().count(), S::Input, u)?;
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(m.error(S::Input, K::InvalidPath("parent root component"), u));
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| m.error(S::Input, K::Io(e), u))?
            .join(path)
    };
    let mut prefix = PathBuf::new();
    for c in absolute.components() {
        prefix.push(c.as_os_str());
        if matches!(c, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&prefix).map_err(|e| m.error(S::Input, K::Io(e), u))?;
        if link(&metadata) {
            return Err(m.error(S::Input, K::InvalidPath("linked root or ancestor"), u));
        }
    }
    let canonical = fs::canonicalize(absolute).map_err(|e| m.error(S::Input, K::Io(e), u))?;
    if !fs::metadata(&canonical)
        .map_err(|e| m.error(S::Input, K::Io(e), u))?
        .is_dir()
    {
        return Err(m.error(S::Input, K::InvalidPath("root is not a directory"), u));
    }
    Ok(canonical)
}
fn path_prefix(a: &Path, b: &Path) -> bool {
    let mut bb = b.components();
    a.components().all(|x| {
        bb.next().is_some_and(|y| {
            #[cfg(windows)]
            {
                x.as_os_str() == y.as_os_str()
                    || x.as_os_str()
                        .to_str()
                        .zip(y.as_os_str().to_str())
                        .is_some_and(|(x, y)| x.eq_ignore_ascii_case(y))
            }
            #[cfg(not(windows))]
            {
                x == y
            }
        })
    })
}
pub(crate) fn roots(
    d: ApplicationDirectories<'_>,
    m: &mut Meter,
    u: ApplicationUsage,
) -> Result<[PathBuf; 2]> {
    let a = root(d.platform, m, u)?;
    let b = root(d.classes, m, u)?;
    if path_prefix(&a, &b) || path_prefix(&b, &a) {
        return Err(m.error(S::Input, K::OverlappingRoots, u));
    }
    Ok([a, b])
}
fn folded(v: u16) -> u16 {
    if (65..=90).contains(&v) { v + 32 } else { v }
}
fn cmp(a: &[u16], b: &[u16], fold: bool) -> Ordering {
    if fold {
        a.iter()
            .copied()
            .map(folded)
            .cmp(b.iter().copied().map(folded))
    } else {
        a.cmp(b)
    }
}
fn order(
    rows: &[Leaf],
    indices: &mut Vec<usize>,
    buffer: &mut Vec<usize>,
    fold: bool,
    m: &mut Meter,
    u: ApplicationUsage,
) -> Result<()> {
    let n = rows.len();
    m.work(n as u64, S::Input, u)?;
    let log = if n < 2 {
        u64::from(n != 0)
    } else {
        u64::from(usize::BITS - (n - 1).leading_zeros())
    };
    let max = rows.iter().map(|r| r.units.len()).max().unwrap_or(1).max(1);
    let cost = (n as u64)
        .checked_mul(log)
        .and_then(|v| v.checked_mul(max as u64))
        .ok_or_else(|| {
            m.error(
                S::Input,
                K::AccountingOverflow {
                    resource: R::InventoryWork,
                },
                u,
            )
        })?;
    m.inventory(cost, u)?;
    let mut width = 1;
    while width < n {
        let mut start = 0;
        while start < n {
            let mid = start.saturating_add(width).min(n);
            let end = mid.saturating_add(width).min(n);
            let (mut a, mut b) = (start, mid);
            for out in &mut buffer[start..end] {
                if b == end
                    || (a < mid
                        && cmp(&rows[indices[a]].units, &rows[indices[b]].units, fold)
                            != Ordering::Greater)
                {
                    *out = indices[a];
                    a += 1
                } else {
                    *out = indices[b];
                    b += 1
                }
            }
            start = end
        }
        std::mem::swap(indices, buffer);
        width = width.checked_mul(2).unwrap_or(n)
    }
    Ok(())
}
fn collisions(rows: &[Leaf], indices: &[usize], m: &mut Meter, u: ApplicationUsage) -> Result<()> {
    for pair in indices.windows(2) {
        m.work(1, S::Input, u)?;
        let units = rows[pair[0]]
            .units
            .len()
            .checked_add(rows[pair[1]].units.len())
            .ok_or_else(|| {
                m.error(
                    S::Input,
                    K::AccountingOverflow {
                        resource: R::Utf16Units,
                    },
                    u,
                )
            })?;
        m.utf(units, S::Input, u)?;
        if cmp(&rows[pair[0]].units, &rows[pair[1]].units, true) == Ordering::Equal {
            return Err(m.error(S::Input, K::DuplicatePath, u));
        }
    }
    Ok(())
}
pub(crate) fn inventory(
    root: &Path,
    role: InputRole,
    m: &mut Meter,
    u: &mut ApplicationUsage,
) -> Result<Vec<Leaf>> {
    let result = (|| {
        let mut rows = Vec::<Leaf>::new();
        let mut pending = Vec::<(PathBuf, String, u64, u64)>::new();
        m.retain(1, S::Input, *u)?;
        pending.try_reserve(1).map_err(|_| m.alloc(S::Input, *u))?;
        pending.push((root.to_owned(), String::new(), 0, 0));
        while let Some((folder, parent, depth, parent_units)) = pending.pop() {
            for entry in fs::read_dir(&folder).map_err(|e| m.error(S::Input, K::Io(e), *u))? {
                let entry = entry.map_err(|e| m.error(S::Input, K::Io(e), *u))?;
                let total = u
                    .platform
                    .entries
                    .checked_add(u.application.entries)
                    .ok_or_else(|| {
                        m.error(
                            S::Input,
                            K::AccountingOverflow {
                                resource: R::Entries,
                            },
                            *u,
                        )
                    })?;
                Meter::add(total, 1, m.limits.max_entries, R::Entries)
                    .map_err(|e| m.error(S::Input, e, *u))?;
                match role {
                    InputRole::Platform => u.platform.entries += 1,
                    InputRole::Application => u.application.entries += 1,
                };
                m.work(1, S::Input, *u)?;
                let name = entry.file_name();
                let name = name.to_str().ok_or_else(|| {
                    m.error(
                        S::Input,
                        K::InvalidPath("unsupported filename encoding"),
                        *u,
                    )
                })?;
                m.work(name.len() as u64, S::Input, *u)?;
                let name_units = name.encode_utf16().count() as u64;
                m.utf(name_units as usize, S::Input, *u)?;
                if !component(name) {
                    return Err(m.error(S::Input, K::InvalidPath("nonportable component"), *u));
                }
                let count = parent_units
                    .checked_add(u64::from(!parent.is_empty()))
                    .and_then(|v| v.checked_add(name_units))
                    .ok_or_else(|| {
                        m.error(
                            S::Input,
                            K::AccountingOverflow {
                                resource: R::PathUnits,
                            },
                            *u,
                        )
                    })?;
                Meter::add(0, count, m.limits.max_path_units, R::PathUnits)
                    .map_err(|e| m.error(S::Input, e, *u))?;
                let total = u
                    .platform
                    .path_units
                    .checked_add(u.application.path_units)
                    .ok_or_else(|| {
                        m.error(
                            S::Input,
                            K::AccountingOverflow {
                                resource: R::TotalPathUnits,
                            },
                            *u,
                        )
                    })?;
                Meter::add(
                    total,
                    count,
                    m.limits.max_total_path_units,
                    R::TotalPathUnits,
                )
                .map_err(|e| m.error(S::Input, e, *u))?;
                let child_depth = depth.checked_add(1).ok_or_else(|| {
                    m.error(S::Input, K::AccountingOverflow { resource: R::Depth }, *u)
                })?;
                Meter::add(0, child_depth, m.limits.max_depth, R::Depth)
                    .map_err(|e| m.error(S::Input, e, *u))?;
                match role {
                    InputRole::Platform => u.platform.path_units += count,
                    InputRole::Application => u.application.path_units += count,
                };
                m.utf(count as usize, S::Input, *u)?;
                let mut relative = String::new();
                let bytes = parent
                    .len()
                    .checked_add(usize::from(!parent.is_empty()))
                    .and_then(|v| v.checked_add(name.len()))
                    .ok_or_else(|| {
                        m.error(
                            S::Input,
                            K::AccountingOverflow {
                                resource: R::PathUnits,
                            },
                            *u,
                        )
                    })?;
                relative
                    .try_reserve(bytes)
                    .map_err(|_| m.alloc(S::Input, *u))?;
                relative.push_str(&parent);
                if !parent.is_empty() {
                    relative.push('/')
                }
                relative.push_str(name);
                let metadata = fs::symlink_metadata(entry.path())
                    .map_err(|e| m.error(S::Input, K::Io(e), *u))?;
                if link(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
                    return Err(m.error(
                        S::Input,
                        K::InvalidPath("link/reparse or special entry"),
                        *u,
                    ));
                }
                if metadata.is_dir() {
                    m.retain(1, S::Input, *u)?;
                    pending.try_reserve(1).map_err(|_| m.alloc(S::Input, *u))?;
                    let mut prefix = String::new();
                    prefix
                        .try_reserve(relative.len())
                        .map_err(|_| m.alloc(S::Input, *u))?;
                    prefix.push_str(&relative);
                    pending.push((entry.path(), prefix, child_depth, count));
                }
                m.retain(1, S::Input, *u)?;
                rows.try_reserve(1).map_err(|_| m.alloc(S::Input, *u))?;
                let mut units = Vec::new();
                units
                    .try_reserve(count as usize)
                    .map_err(|_| m.alloc(S::Input, *u))?;
                units.extend(relative.encode_utf16());
                rows.push(Leaf {
                    path: entry.path(),
                    class_file: metadata.is_file() && name.ends_with(".class"),
                    relative,
                    units,
                });
            }
        }
        let mut indices = Vec::new();
        let mut buffer = Vec::new();
        m.retain(rows.len(), S::Input, *u)?;
        indices
            .try_reserve(rows.len())
            .map_err(|_| m.alloc(S::Input, *u))?;
        indices.extend(0..rows.len());
        m.retain(rows.len(), S::Input, *u)?;
        buffer
            .try_reserve(rows.len())
            .map_err(|_| m.alloc(S::Input, *u))?;
        buffer.resize(rows.len(), 0);
        order(&rows, &mut indices, &mut buffer, true, m, *u)?;
        collisions(&rows, &indices, m, *u)?;
        order(&rows, &mut indices, &mut buffer, false, m, *u)?;
        // Move each owned row exactly once, using checked permutation positions.
        m.retain(rows.len(), S::Input, *u)?;
        let mut slots = Vec::new();
        slots
            .try_reserve(rows.len())
            .map_err(|_| m.alloc(S::Input, *u))?;
        slots.extend(rows.into_iter().map(Some));
        let mut sorted = Vec::new();
        m.retain(indices.len(), S::Input, *u)?;
        sorted
            .try_reserve(indices.len())
            .map_err(|_| m.alloc(S::Input, *u))?;
        for id in indices {
            m.work(1, S::Input, *u)?;
            let row = slots
                .get_mut(id)
                .and_then(Option::take)
                .ok_or_else(|| m.error(S::Input, K::Invariant("invalid path permutation"), *u))?;
            if row.class_file {
                sorted.push(row)
            }
        }
        Ok(sorted)
    })();
    result.map_err(|mut e| {
        e.role = Some(role);
        e
    })
}
pub(crate) fn read(
    leaf: &Leaf,
    role: InputRole,
    m: &Meter,
    u: &mut ApplicationUsage,
) -> Result<Vec<u8>> {
    let result = (|| {
        let metadata =
            fs::symlink_metadata(&leaf.path).map_err(|e| m.error(S::Definition, K::Io(e), *u))?;
        if link(&metadata) || !metadata.is_file() {
            return Err(m.error(S::Definition, K::InputChanged, *u));
        }
        let file = File::open(&leaf.path).map_err(|e| m.error(S::Definition, K::Io(e), *u))?;
        let before = file
            .metadata()
            .map_err(|e| m.error(S::Definition, K::Io(e), *u))?;
        if !before.is_file() {
            return Err(m.error(S::Definition, K::InputChanged, *u));
        }
        let size = before.len();
        Meter::add(0, size, m.limits.max_file_bytes, R::FileBytes)
            .map_err(|e| m.error(S::Definition, e, *u))?;
        let paid = u
            .platform
            .class_bytes
            .checked_add(u.application.class_bytes)
            .ok_or_else(|| {
                m.error(
                    S::Definition,
                    K::AccountingOverflow {
                        resource: R::InputBytes,
                    },
                    *u,
                )
            })?;
        Meter::add(paid, size, m.limits.max_input_bytes, R::InputBytes)
            .map_err(|e| m.error(S::Definition, e, *u))?;
        let mut reader = file;
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let remaining = size
                .checked_sub(bytes.len() as u64)
                .ok_or_else(|| m.error(S::Definition, K::InputChanged, *u))?;
            let n = usize::try_from(remaining.min(8191) + 1)
                .map_err(|_| m.error(S::Definition, K::InputChanged, *u))?;
            let got = reader
                .read(&mut chunk[..n])
                .map_err(|e| m.error(S::Definition, K::Io(e), *u))?;
            if got == 0 {
                break;
            }
            if got as u64 > remaining {
                return Err(m.error(S::Definition, K::InputChanged, *u));
            }
            bytes
                .try_reserve(got)
                .map_err(|_| m.alloc(S::Definition, *u))?;
            bytes.extend_from_slice(&chunk[..got]);
        }
        let after = reader
            .metadata()
            .map_err(|e| m.error(S::Definition, K::Io(e), *u))?;
        if size != bytes.len() as u64
            || after.len() != size
            || before.modified().ok() != after.modified().ok()
            || !after.is_file()
        {
            return Err(m.error(S::Definition, K::InputChanged, *u));
        }
        let usage = match role {
            InputRole::Platform => &mut u.platform,
            InputRole::Application => &mut u.application,
        };
        usage.class_files += 1;
        usage.class_bytes += size;
        Ok(bytes)
    })();
    result.map_err(|mut e| {
        e.role = Some(role);
        e.relative_path = Some(leaf.relative.clone());
        e
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn leaf(s: &str) -> Leaf {
        Leaf {
            path: PathBuf::from(s),
            relative: s.to_owned(),
            units: s.encode_utf16().collect(),
            class_file: true,
        }
    }
    #[test]
    fn merge_envelope_is_inclusive_and_rejected_batch_keeps_scalar_prefix() {
        let rows = vec![leaf("abc"), leaf("abA")];
        for limit in [7, 8] {
            let mut m = Meter::new(crate::ImageLimits {
                max_inventory_work: limit,
                ..Default::default()
            });
            let mut ids = vec![0, 1];
            let mut temp = vec![0, 0];
            let result = order(
                &rows,
                &mut ids,
                &mut temp,
                false,
                &mut m,
                Default::default(),
            );
            if limit == 8 {
                result.unwrap();
                assert_eq!(ids, [1, 0]);
                assert_eq!(m.usage.work_units, 8);
                assert_eq!(m.usage.inventory_work, 6)
            } else {
                assert!(result.is_err());
                assert_eq!(m.usage.work_units, 2);
                assert_eq!(m.usage.inventory_work, 0)
            }
        }
    }
    #[test]
    fn adjacent_folded_collision_funds_both_full_operands_before_equality() {
        let rows = vec![leaf("A"), leaf("a")];
        let mut m = Meter::new(crate::ImageLimits {
            max_inventory_work: 3,
            max_utf16_units: 2,
            ..Default::default()
        });
        assert!(matches!(
            collisions(&rows, &[0, 1], &mut m, Default::default())
                .unwrap_err()
                .kind,
            K::DuplicatePath
        ));
        assert_eq!(m.usage.utf16_units, 2);
        assert_eq!(m.usage.work_units, 3);
        let mut m = Meter::new(crate::ImageLimits {
            max_utf16_units: 1,
            ..Default::default()
        });
        assert!(matches!(
            collisions(&rows, &[0, 1], &mut m, Default::default())
                .unwrap_err()
                .kind,
            K::ResourceLimit {
                resource: R::Utf16Units,
                ..
            }
        ));
        assert_eq!(m.usage.work_units, 1);
        assert_eq!(m.usage.utf16_units, 0);
    }
}
