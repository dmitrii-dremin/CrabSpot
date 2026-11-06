//! Observe genuine Java equality and bootstrap assertion calls through the public launcher.
use crabspot_interpreter::ExecutionErrorKind;
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationErrorKind, ApplicationInvocation,
    ApplicationOptions, ApplicationStringEqualityProfile, ApplicationUsage,
    BootstrapAssertionPolicy, StaticIntObservation, StringEqualityMode,
    run_directory_application_with_string_equality,
};
use crabspot_runtime::{
    HeapError, InstanceError, ReferenceType, StringBodyError, StringBodyErrorKind,
};
use std::{env, error::Error, path::PathBuf};

fn cause(mut error: &(dyn Error + 'static)) -> Result<String, Box<dyn Error>> {
    loop {
        if let Some(body) = error.downcast_ref::<StringBodyError>() {
            match &body.kind {
                StringBodyErrorKind::UnsupportedShape => return Ok("UnsupportedShape".into()),
                StringBodyErrorKind::InvalidBinding(why) => {
                    return Ok(format!("InvalidBinding {why}"));
                }
                StringBodyErrorKind::Limit {
                    resource,
                    limit,
                    required,
                } => return Ok(format!("{resource:?} {limit}/{required}")),
                StringBodyErrorKind::UnsupportedAssertionTarget { represented } => {
                    return Ok(match represented {
                        ReferenceType::Class(_) => "UnsupportedAssertionTarget class",
                        ReferenceType::Array { .. } => "UnsupportedAssertionTarget array",
                    }
                    .into());
                }
                _ => {}
            }
        }
        if let Some(HeapError::ResourceLimit {
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
            .ok_or_else(|| format!("unexpected cause {error}"))?;
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform")?);
    let classes = PathBuf::from(args.next().ok_or("missing classes")?);
    let mut args = args.map(|s| s.into_string().map_err(|_| "invalid fixture option"));
    let entry = args.next().ok_or("missing entry")??;
    let shape = args.next().ok_or("missing input shape")??;
    let arguments: Vec<Vec<u16>> = match shape.as_str() {
        "text" => vec![
            vec![65, 0, 255],
            vec![65, 0, 255],
            vec![65, 0, 254],
            vec![65, 0x20ac, 0xdfff],
            vec![65, 0x20ac, 0xdfff],
            vec![65, 0x20ac, 0xdffe],
            vec![],
            vec![],
            vec![65, 0x20ac],
        ],
        "none" => vec![],
        "one" => vec![vec![]],
        "two" => vec![vec![], vec![]],
        "alias" => vec![vec![113; 128]],
        "equal" | "first" | "last" => {
            let mut right = vec![113; 128];
            if shape == "first" {
                right[0] = 120;
            }
            if shape == "last" {
                right[127] = 120;
            }
            vec![vec![113; 128], right]
        }
        "cross-coder" => vec![vec![0xfe], vec![0xdfff]],
        _ => return Err("unknown input shape".into()),
    };
    let fields: &[&str] = match entry.as_str() {
        "app.TextEquality" => &["flags", "calls"],
        "app.BootstrapStatus" => &["status", "phase"],
        "app.Policy" => &["status", "effects"],
        "probe.Compare" => &["phase", "same"],
        "query.Query" => &["phase", "enabled", "initialized"],
        _ => return Err("unknown observer".into()),
    };
    let queries: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: &entry,
            field,
        })
        .collect();
    let refs: Vec<_> = arguments.iter().map(Vec::as_slice).collect();
    let mut options = ApplicationOptions::default();
    let mut profile = ApplicationStringEqualityProfile::default();
    for option in args {
        let option = option?;
        match option.as_str() {
            "enabled" => profile.bootstrap_assertions = BootstrapAssertionPolicy::Enabled,
            "strict" => profile.mode = StringEqualityMode::RequireCompactEquality,
            _ => {
                let (key, value) = option.split_once('=').ok_or("expected resource=limit")?;
                match key {
                    "work" => profile.bodies.backing.max_work_units = value.parse()?,
                    "discovery" => profile.discovery.max_work_units = value.parse()?,
                    "arrays" => options.heap.max_arrays = value.parse()?,
                    "code" => options.calls.max_prepared_code_bytes = value.parse()?,
                    "frames" => options.calls.max_frames = value.parse()?,
                    "handlers" => options.exceptions.max_prepared_handlers = value.parse()?,
                    "fuel" => options.execution.instruction_budget = value.parse()?,
                    _ => return Err("unknown option".into()),
                }
            }
        }
    }
    let result = run_directory_application_with_string_equality(
        ApplicationDirectories {
            platform: &platform,
            classes: &classes,
        },
        &entry,
        &queries,
        options,
        ApplicationInvocation {
            arguments: &refs,
            ..Default::default()
        },
        profile,
    );
    match result {
        Err(error) => {
            let why = if matches!(
                error.kind,
                ApplicationErrorKind::UnsupportedPlatformReference
            ) {
                format!(
                    "UnsupportedPlatformReference cp={}",
                    error.cp_index.ok_or("missing import CP index")?
                )
            } else {
                cause(error.as_ref())?
            };
            println!("image {:?} {why}", error.stage);
            usage(error.usage);
        }
        Ok(run) => {
            match run.completion {
                ApplicationCompletion::Returned => println!("returned"),
                ApplicationCompletion::ExecutionFailed { phase, error } => {
                    let why = match &error.kind {
                        ExecutionErrorKind::InstructionBudgetExhausted { limit } => {
                            format!("Fuel {limit}")
                        }
                        ExecutionErrorKind::PreparedCodeLimit { limit, required } => {
                            format!("Code {limit}/{required}")
                        }
                        ExecutionErrorKind::FrameLimit { limit, required } => {
                            format!("Frames {limit}/{required}")
                        }
                        ExecutionErrorKind::ExceptionHandlerLimit { limit, required } => {
                            format!("Handlers {limit}/{required}")
                        }
                        _ => cause(error.as_ref())?,
                    };
                    let site = error.method.as_ref().ok_or("missing method")?;
                    let pc = error
                        .offset
                        .map_or_else(|| "entry".to_owned(), |n| n.to_string());
                    let via = error
                        .call_site
                        .as_ref()
                        .map(|s| format!(" via {}@{}", s.method.name.escaped(), s.offset))
                        .unwrap_or_default();
                    if site.name.escaped() == "<clinit>" {
                        let owner = site.owner.ok_or("missing initializer owner")?;
                        let caller = error
                            .call_site
                            .as_ref()
                            .ok_or("missing initializer caller")?;
                        if error.initialization_class != Some(owner)
                            || caller.method.owner != Some(owner)
                            || caller.method.name.escaped() != "desiredAssertionStatus"
                            || site.descriptor.escaped() != "()V"
                        {
                            return Err(
                                "Class initializer and assertion caller identities differ".into()
                            );
                        }
                    }
                    if let Some(owner) = error.initialization_class {
                        if Some(owner) != site.owner || site.name.escaped() != "<clinit>" {
                            return Err("failure attributed to a different initializer".into());
                        }
                    }
                    println!("{phase:?} {}@{pc}{via} {why}", site.name.escaped());
                }
                _ => return Err("unexpected guest exception".into()),
            }
            println!("fields={:?}", run.static_ints);
            usage(run.usage);
        }
    }
    Ok(())
}

fn usage(usage: ApplicationUsage) {
    let b = usage.string_bodies;
    let e = usage.string_equality;
    println!(
        "proved={} witnesses={} admitted={} argv={} args={} evaluations={} backings={} bytes={} work={} discovery={} compact={} registered={} queries={}",
        usage.proved_classes,
        usage.preparation_witnesses,
        usage.admitted_executable_classes,
        usage.argument_arrays,
        usage.argument_strings,
        usage.engine_evaluations,
        b.backings,
        b.backing_bytes,
        b.work_units,
        usage.string_equality_discovery.work_units,
        usage.string_body_discovery.work_units,
        u8::from(e.natives_registered),
        e.assertion_queries
    );
}
