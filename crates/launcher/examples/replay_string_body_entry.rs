//! Run an archived Java-body scenario through the public directory entry point.
use crabspot_interpreter::ExecutionErrorKind;
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationErrorKind, ApplicationInvocation,
    ApplicationOptions, ApplicationStringBodyProfile, ApplicationUsage, StaticIntObservation,
    StringBodyMode, run_directory_application_with_string_bodies,
};
use crabspot_runtime::{HeapError, InstanceError, StringBodyError, StringBodyErrorKind};
use std::{env, error::Error, path::PathBuf};

fn reason(mut error: &(dyn Error + 'static)) -> Result<String, Box<dyn Error>> {
    loop {
        if let Some(body) = error.downcast_ref::<StringBodyError>() {
            match &body.kind {
                StringBodyErrorKind::Limit {
                    resource,
                    limit,
                    required,
                } => return Ok(format!("{resource:?} {limit}/{required}")),
                StringBodyErrorKind::UnsupportedShape => return Ok("UnsupportedShape".into()),
                StringBodyErrorKind::InvalidBinding(why) => {
                    return Ok(format!("InvalidBinding {why}"));
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

fn usage(u: ApplicationUsage) {
    let b = u.string_bodies;
    println!(
        "discovery={} work={} backings={} bytes={} proved={} witnesses={} admitted={} argv={} args={} evaluations={}",
        u.string_body_discovery.work_units,
        b.work_units,
        b.backings,
        b.backing_bytes,
        u.proved_classes,
        u.preparation_witnesses,
        u.admitted_executable_classes,
        u.argument_arrays,
        u.argument_strings,
        u.engine_evaluations
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform")?);
    let classes = PathBuf::from(args.next().ok_or("missing application")?);
    let mut args = args.map(|s| s.into_string().map_err(|_| "invalid fixture option"));
    let entry = args.next().ok_or("missing entry")??;
    let shape = args.next().ok_or("missing argument set")??;
    let arguments: &[&[u16]] = match shape.as_str() {
        "none" => &[],
        "empty" => &[&[]],
        "two" => &[&[], &[0, 0xdfff, 0x20ac]],
        "latin" => &[&[65, 0, 255]],
        "wide" => &[&[0xd83d, 0xde80]],
        _ => return Err("unknown argument set".into()),
    };
    let fields: &[&str] = match entry.as_str() {
        "compact.Entry" => &[
            "flags",
            "emptyLength",
            "latinLength",
            "mixedLength",
            "isolatedLength",
            "pairLength",
            "argumentLength",
        ],
        "compact.EmptyOnly" => &["flags"],
        "budget.Entry" => &["phase", "units", "empty", "stable"],
        "peer.Host" => &["phase"],
        "words.Entry" => &["flags", "hash", "units"],
        _ => return Err("unknown observation set".into()),
    };
    let queries: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: &entry,
            field,
        })
        .collect();
    let mut options = ApplicationOptions::default();
    let mut profile = ApplicationStringBodyProfile::default();
    for option in args {
        let option = option?;
        if option == "strict" {
            profile.mode = StringBodyMode::RequireCompact;
            continue;
        }
        let (key, raw) = option.split_once('=').ok_or("expected limit=value")?;
        match key {
            "discovery" => profile.discovery.max_work_units = raw.parse()?,
            "work" => profile.backing.max_work_units = raw.parse()?,
            "arrays" => options.heap.max_arrays = raw.parse()?,
            "length" => options.heap.max_array_length = raw.parse()?,
            "fuel" => options.execution.instruction_budget = raw.parse()?,
            "frames" => options.calls.max_frames = raw.parse()?,
            "code" => options.calls.max_prepared_code_bytes = raw.parse()?,
            _ => return Err("unknown ceiling".into()),
        }
    }
    let invocation = ApplicationInvocation {
        arguments,
        ..Default::default()
    };
    let dirs = ApplicationDirectories {
        platform: &platform,
        classes: &classes,
    };
    match run_directory_application_with_string_bodies(
        dirs, &entry, &queries, options, invocation, profile,
    ) {
        Err(error) => {
            let why = match error.kind {
                ApplicationErrorKind::UnsupportedPlatformReference => {
                    "UnsupportedPlatformReference".into()
                }
                ApplicationErrorKind::MissingApplicationDependency => {
                    "MissingApplicationDependency".into()
                }
                _ => reason(error.as_ref())?,
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
                        ExecutionErrorKind::UnsupportedVmException(kind) => format!("{kind:?}"),
                        ExecutionErrorKind::FrameLimit { limit, required } => {
                            format!("Frames {limit}/{required}")
                        }
                        ExecutionErrorKind::PreparedCodeLimit { limit, required } => {
                            format!("CodeBytes {limit}/{required}")
                        }
                        _ => reason(error.as_ref())?,
                    };
                    let site = error.method.as_ref().ok_or("missing method")?;
                    let method_name = site.name.escaped();
                    if method_name == "<clinit>" {
                        let owner = site.owner.ok_or("missing initializer owner")?;
                        let caller = error
                            .call_site
                            .as_ref()
                            .ok_or("missing initializer caller")?;
                        if Some(owner) != error.initialization_class
                            || caller.method.name.escaped() != "coder"
                            || caller.method.owner != Some(owner)
                        {
                            return Err("initializer and String.coder owners differ".into());
                        }
                    }
                    // Preparing a new frame can fail before it executes an instruction.
                    let pc = error
                        .offset
                        .map(|offset| offset.to_string())
                        .unwrap_or_else(|| "entry".into());
                    let via = error
                        .call_site
                        .as_ref()
                        .map(|c| format!(" via {}@{}", c.method.name.escaped(), c.offset))
                        .unwrap_or_default();
                    println!("{phase:?} {method_name}@{pc}{via} {why}");
                }
                ApplicationCompletion::SuppliedUncaught { class_name, .. } => {
                    return Err(format!("unexpected {}", class_name.escaped()).into());
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
