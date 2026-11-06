//! Observe genuine Throwable applications through the public directory API.
use crabspot_interpreter::ExecutionErrorKind;
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationErrorKind, ApplicationInvocation,
    ApplicationOptions, ApplicationThrowableProfile, ApplicationUsage, StaticIntObservation,
    ThrowableMode, run_directory_application_with_throwables,
};
use crabspot_runtime::{ThrowableAccessError, ThrowableErrorKind};
use std::{env, error::Error, path::PathBuf};

fn cause(mut error: &(dyn Error + 'static)) -> Result<String, Box<dyn Error>> {
    loop {
        if let Some(error) = error.downcast_ref::<ThrowableAccessError>() {
            match &error.kind {
                ThrowableErrorKind::UnsupportedShape => return Ok("UnsupportedShape".into()),
                ThrowableErrorKind::InvalidBinding(reason) => {
                    return Ok(format!("InvalidBinding {reason}"));
                }
                ThrowableErrorKind::Limit {
                    resource,
                    limit,
                    required,
                } => {
                    return Ok(format!("{resource:?} {limit}/{required}"));
                }
                _ => {}
            }
        }
        error = error
            .source()
            .ok_or_else(|| format!("unexpected diagnostic: {error}"))?;
    }
}

fn usage(value: ApplicationUsage) {
    println!(
        "proved={} witnesses={} admitted={} argv={} arguments={} evaluations={} discovery={} snapshots={} frames={} backtraces={} trace_work={} started={} produced={}",
        value.proved_classes,
        value.preparation_witnesses,
        value.admitted_executable_classes,
        value.argument_arrays,
        value.argument_strings,
        value.engine_evaluations,
        value.throwable_discovery.work_units,
        value.traces.snapshots,
        value.traces.frames,
        value.traces.backtraces,
        value.traces.work_units,
        value.vm_exceptions.started,
        value.vm_exceptions.produced,
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform directory")?);
    let classes = PathBuf::from(args.next().ok_or("missing application directory")?);
    let entry = args
        .next()
        .and_then(|v| v.into_string().ok())
        .ok_or("missing entry name")?;
    let fields: &[&str] = match entry.as_str() {
        "app.ThrowableMain" => &["flags", "arithmeticCases", "localCases", "completed"],
        "app.ThrowableUncaught" | "app.ThrowableTraceAccess" => &["before", "after"],
        "app.ThrowableRefilled" | "app.Bare" => &[],
        _ => return Err("unknown fixture entry".into()),
    };
    let observations: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: &entry,
            field,
        })
        .collect();
    let mut profile = ApplicationThrowableProfile::default();
    let mut wrong = false;
    for option in args {
        let option = option.into_string().map_err(|_| "invalid fixture option")?;
        match option.as_str() {
            "wrong" => wrong = true,
            "strict" => profile.mode = ThrowableMode::RequireThrowable,
            "disabled" => profile.mode = ThrowableMode::Disabled,
            _ => {
                let (key, text) = option.split_once('=').ok_or("invalid limit option")?;
                let value: u64 = text.parse()?;
                match key {
                    "vm" => profile.vm_exceptions.max_materializations = value,
                    "snapshots" => profile.traces.max_snapshots = value,
                    "frames" => profile.traces.max_frames = value,
                    "depth" => profile.traces.max_frames_per_snapshot = value,
                    "trace-work" => profile.traces.max_work_units = value,
                    "discovery" => profile.discovery.max_work_units = value,
                    _ => return Err("unknown limit option".into()),
                }
            }
        }
    }
    let wrong_units: Vec<u16> = "wrong".encode_utf16().collect();
    let arguments: Vec<&[u16]> = if wrong { vec![&wrong_units] } else { vec![] };
    let result = run_directory_application_with_throwables(
        ApplicationDirectories {
            platform: &platform,
            classes: &classes,
        },
        &entry,
        &observations,
        ApplicationOptions::default(),
        ApplicationInvocation {
            arguments: &arguments,
            ..ApplicationInvocation::default()
        },
        profile,
    );
    let run = match result {
        Err(error) => {
            let why = match &error.kind {
                ApplicationErrorKind::UnsupportedPlatformReference => {
                    "UnsupportedPlatformReference".into()
                }
                ApplicationErrorKind::Throwable(_) => cause(error.as_ref())?,
                _ => return Err(error),
            };
            println!("image {:?} {why}", error.stage);
            usage(error.usage);
            return Ok(());
        }
        Ok(run) => run,
    };
    match run.completion {
        ApplicationCompletion::Returned => println!("returned"),
        ApplicationCompletion::Uncaught {
            phase,
            class_name,
            location,
        } => {
            let location = location.map_or_else(
                || "none".into(),
                |head| {
                    format!(
                        "{}.{}{}@{}",
                        head.class_name.escaped(),
                        head.method_name.escaped(),
                        head.descriptor.escaped(),
                        head.offset,
                    )
                },
            );
            println!("{phase:?} uncaught {} at {location}", class_name.escaped());
        }
        ApplicationCompletion::ExecutionFailed { phase, error } => {
            let why = match &error.kind {
                ExecutionErrorKind::VmExceptionLimit { limit, required } => {
                    format!("VmExceptionLimit {limit}/{required}")
                }
                ExecutionErrorKind::UnsupportedVmException(kind) => {
                    format!("UnsupportedVmException {kind:?}")
                }
                ExecutionErrorKind::UnsupportedMethod(reason) => {
                    format!("UnsupportedMethod {reason}")
                }
                ExecutionErrorKind::ThrowableAccess(_) => cause(error.as_ref())?,
                _ => return Err(error),
            };
            let method = error.method.as_ref().ok_or("missing execution method")?;
            let pc = error
                .offset
                .map_or_else(|| "entry".into(), |value| value.to_string());
            let call = error.call_site.as_ref().map_or_else(
                || "none".into(),
                |site| {
                    format!(
                        "{}{}@{}",
                        site.method.name.escaped(),
                        site.method.descriptor.escaped(),
                        site.offset
                    )
                },
            );
            let fault = error.vm_exception.as_ref().map_or_else(
                || "none".into(),
                |context| {
                    format!(
                        "{:?}@{}:{:?}:same-method={}",
                        context.origin.kind,
                        context.origin.pc,
                        context.phase,
                        method.owner == Some(context.origin.method.owner())
                            && method.index == context.origin.method.index()
                    )
                },
            );
            let owner_present = method.owner.is_some();
            let same_owner = error
                .call_site
                .as_ref()
                .is_some_and(|site| owner_present && site.method.owner == method.owner);
            let distinct_method = error
                .call_site
                .as_ref()
                .is_some_and(|site| site.method.index != method.index);
            println!(
                "{phase:?} failed {}{}@{pc} via={call} fault={fault} owner={owner_present} call-owner-same={same_owner} call-method-distinct={distinct_method} {why}",
                method.name.escaped(),
                method.descriptor.escaped()
            );
        }
        other => return Err(format!("unexpected legacy completion: {other:?}").into()),
    }
    println!("fields={:?}", run.static_ints);
    usage(run.usage);
    Ok(())
}
