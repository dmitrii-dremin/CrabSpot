//! Observe String fixtures using the public UTF-16 directory entry point.
use crabspot_interpreter::MonitorError;
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationErrorKind, ApplicationInvocation,
    ApplicationUsage, StaticIntObservation, run_directory_application_with_arguments,
};
use crabspot_runtime::{InstanceError, StringError};
use std::{env, error::Error, path::PathBuf};

fn cause(mut error: &(dyn Error + 'static)) -> Result<String, Box<dyn Error>> {
    loop {
        if let Some(StringError::Limit {
            resource,
            limit,
            required,
        }) = error.downcast_ref()
        {
            return Ok(format!("{resource:?} {limit}/{required}"));
        }
        if let Some(MonitorError::Limit {
            resource,
            limit,
            required,
        }) = error.downcast_ref()
        {
            return Ok(format!("{resource:?} {limit}/{required}"));
        }
        if let Some(InstanceError::OpaqueStringObject { .. }) = error.downcast_ref() {
            return Ok("OpaqueStringObject".into());
        }
        error = error
            .source()
            .ok_or_else(|| format!("unrecognized error: {error}"))?;
    }
}

fn usage(u: ApplicationUsage) {
    let s = u.strings;
    println!(
        "arrays={} args={} evaluations={} objects={} units={} interned={} sites={} work={} proved={}",
        u.argument_arrays,
        u.argument_strings,
        u.engine_evaluations,
        s.objects,
        s.retained_units,
        s.interned,
        s.literal_sites,
        s.work_units,
        u.proved_classes
    );
}

fn argument_units(case: &str) -> Result<Vec<Vec<u16>>, Box<dyn Error>> {
    let strs: &[&str] = match case {
        "empty" => &[],
        "normal" => &["crab", "crab", "", "left right"],
        "two" => &["same", "same"],
        "three" => &["same", "same", ""],
        "transport8" | "transport9" => &[
            "repeat",
            "repeat",
            "",
            "",
            "--",
            "--platform",
            "left right",
            "\u{20ac}\u{1f680}",
        ],
        "zero" => &[""],
        "latin" => return Ok(vec![vec![0, 1, 255]]),
        "isolated-both" => return Ok(vec![vec![0xdfff, 0, 0xd800]]),
        "paired" => return Ok(vec![vec![0xd83d, 0xde80]]),
        "composed" => return Ok(vec![vec![233]]),
        "decomposed" => return Ok(vec![vec![101, 769]]),
        "max-mutf" => return Ok(vec![vec![0xdfff; 21845]]),
        _ => return Err("unknown argument fixture".into()),
    };
    let mut units: Vec<Vec<u16>> = strs.iter().map(|s| s.encode_utf16().collect()).collect();
    if case == "transport9" {
        units.push(vec![0, 0xdfff]);
    }
    Ok(units)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform directory")?);
    let classes = PathBuf::from(args.next().ok_or("missing application directory")?);
    let mut args = args.map(|a| {
        a.into_string()
            .map_err(|_| "expected fixture or limit name")
    });
    let main = args.next().ok_or("missing main")??;
    let case = args.next().ok_or("missing argument fixture")??;
    let owned = argument_units(&case)?;
    let borrowed: Vec<_> = owned.iter().map(Vec::as_slice).collect();
    let mut invocation = ApplicationInvocation {
        arguments: &borrowed,
        ..Default::default()
    };
    for option in args {
        let option = option?;
        let (name, value) = option.split_once('=').ok_or("expected name=value")?;
        let ceiling = match name {
            "args" => &mut invocation.strings.max_arguments,
            "total" => &mut invocation.strings.max_argument_units,
            "each" => &mut invocation.strings.max_units_per_argument,
            "objects" => &mut invocation.strings.strings.max_objects,
            "units" => &mut invocation.strings.strings.max_retained_units,
            "interned" => &mut invocation.strings.strings.max_interned,
            "sites" => &mut invocation.strings.strings.max_literal_sites,
            "work" => &mut invocation.strings.strings.max_work_units,
            "monitors" => &mut invocation.monitors.max_active_monitors,
            "depth" => &mut invocation.monitors.max_recursion,
            _ => return Err("unknown limit".into()),
        };
        *ceiling = value.parse()?;
    }
    let (owner, fields): (&str, &[&str]) = match main.as_str() {
        "texts.Entry" => ("texts.State", &["flags", "initialized", "seen", "argc"]),
        "words.Entry" => (&main, &["flags", "hash", "units"]),
        "bare.Literal" => (&main, &["flags"]),
        "transport.Entry" => (&main, &["count", "flags", "aliases"]),
        "constants.Entry" => (&main, &["NUMBER", "entered", "completed"]),
        "duplicates.Entry" | "sample.Entry" | "locks.Strings" => (&main, &["phase"]),
        _ => return Err("unknown main".into()),
    };
    let mut queries: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: owner,
            field,
        })
        .collect();
    if main == "constants.Entry" {
        queries.push(StaticIntObservation {
            binary_class: "constants.Marks",
            field: "order",
        });
    }
    let dirs = ApplicationDirectories {
        platform: &platform,
        classes: &classes,
    };
    match run_directory_application_with_arguments(
        dirs,
        &main,
        &queries,
        Default::default(),
        invocation,
    ) {
        Err(error) => {
            match &error.kind {
                ApplicationErrorKind::Verification(_) => {
                    println!("image {:?} Verification", error.stage)
                }
                ApplicationErrorKind::Arguments(e) => println!("image {:?} {e:?}", error.stage),
                ApplicationErrorKind::Setup { kind, source } => {
                    println!(
                        "image {:?} {kind:?} {}",
                        error.stage,
                        cause(source.as_ref())?
                    )
                }
                _ => return Err(error),
            }
            usage(error.usage);
        }
        Ok(run) => {
            if run.usage.proved_classes != run.usage.preparation_witnesses
                || run.usage.proved_classes != run.usage.admitted_executable_classes
            {
                return Err("execution image proof mismatch".into());
            }
            match run.completion {
                ApplicationCompletion::Returned => println!("returned"),
                ApplicationCompletion::ExecutionFailed { phase, error } => {
                    let site = match (&error.method, error.offset) {
                        (Some(method), Some(pc)) => format!("{}@{pc}", method.name.escaped()),
                        (None, None) if error.initialization_class.is_some() => {
                            "initialization".into()
                        }
                        _ => return Err("missing or inconsistent failure location".into()),
                    };
                    println!("{phase:?} {site} {}", cause(error.as_ref())?);
                }
                ApplicationCompletion::Uncaught {
                    phase,
                    class_name,
                    location,
                } => {
                    let text = {
                        let head = location.map_or_else(String::new, |h| {
                            format!(
                                " at {}.{}{}@{}",
                                h.class_name.escaped(),
                                h.method_name.escaped(),
                                h.descriptor.escaped(),
                                h.offset
                            )
                        });
                        format!("{phase:?} uncaught {}{head}", class_name.escaped())
                    };
                    println!("{text}");
                }
                ApplicationCompletion::SuppliedUncaught { class_name, .. } => {
                    return Err(format!("unexpected throwable {}", class_name.escaped()).into());
                }
            }
            println!(
                "fields={}",
                run.static_ints
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join("/")
            );
            usage(run.usage);
        }
    }
    Ok(())
}

#[cfg(test)]
mod transport_encoding_tests {
    use super::argument_units;
    #[test]
    fn transport_argument_has_the_original_euro_and_rocket_utf16_units() {
        let eight = argument_units("transport8").unwrap();
        assert_eq!(eight.len(), 8);
        assert_eq!(eight[7], [0x20ac, 0xd83d, 0xde80]);
        assert_eq!(eight.iter().map(Vec::len).sum::<usize>(), 37);
        let nine = argument_units("transport9").unwrap();
        assert_eq!(&nine[..8], eight.as_slice());
        assert_eq!(nine[8], [0, 0xdfff]);
        assert_eq!(nine.iter().map(Vec::len).sum::<usize>(), 39);
    }
}
