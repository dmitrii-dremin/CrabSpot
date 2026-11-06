//! Observe genuine operand exception applications through the public directory API.
use crabspot_interpreter::ExecutionErrorKind;
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationErrorKind, ApplicationInvocation,
    ApplicationOperandExceptionProfile, ApplicationOptions, ApplicationUsage, OperandExceptionMode,
    StaticIntObservation, run_directory_application_with_arguments,
    run_directory_application_with_operand_exceptions, run_directory_application_with_throwables,
};
use crabspot_runtime::{
    HeapError, InstanceError, OperandExceptionError, OperandExceptionErrorKind, StringError,
    ThrowableAccessError, ThrowableErrorKind,
};
use std::{env, error::Error, path::PathBuf};

fn cause(mut error: &(dyn Error + 'static)) -> Result<String, Box<dyn Error>> {
    loop {
        if let Some(error) = error.downcast_ref::<OperandExceptionError>() {
            match &error.kind {
                OperandExceptionErrorKind::UnsupportedShape => return Ok("UnsupportedShape".into()),
                OperandExceptionErrorKind::InvalidBinding(reason) => {
                    return Ok(format!("InvalidBinding {reason}"));
                }
                OperandExceptionErrorKind::Limit {
                    resource,
                    limit,
                    required,
                } => return Ok(format!("{resource:?} {limit}/{required}")),
                _ => {}
            }
        }
        if let Some(StringError::Limit {
            resource,
            limit,
            required,
        }) = error.downcast_ref::<StringError>()
        {
            return Ok(format!("String{resource:?} {limit}/{required}"));
        }
        if let Some(HeapError::ResourceLimit {
            resource,
            limit,
            required,
        }) = error.downcast_ref::<HeapError>()
        {
            return Ok(format!("Heap{resource:?} {limit}/{required}"));
        }
        if let Some(InstanceError::ResourceLimit {
            resource,
            limit,
            required,
        }) = error.downcast_ref::<InstanceError>()
        {
            return Ok(format!("Instance{resource:?} {limit}/{required}"));
        }
        if let Some(crabspot_runtime::FieldAccessError::ResourceLimit {
            resource,
            limit,
            required,
        }) = error.downcast_ref::<crabspot_runtime::FieldAccessError>()
        {
            return Ok(format!("Field{resource:?} {limit}/{required}"));
        }
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
        "proved={} witnesses={} admitted={} argv={} arguments={} evaluations={} discovery={} throwable_discovery={} diag_work={} message_units={} messages={} string_work={} string_objects={} string_units={} snapshots={} frames={} backtraces={} trace_work={} started={} produced={}",
        value.proved_classes,
        value.preparation_witnesses,
        value.admitted_executable_classes,
        value.argument_arrays,
        value.argument_strings,
        value.engine_evaluations,
        value.operand_exception_discovery.work_units,
        value.throwable_discovery.work_units,
        value.operand_diagnostics.work_units,
        value.operand_diagnostics.message_units,
        value.operand_diagnostics.messages,
        value.strings.work_units,
        value.strings.objects,
        value.strings.retained_units,
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
        "app.OperandMain" => &[
            "bounds",
            "negative",
            "stores",
            "casts",
            "arithmetic",
            "checked",
            "positives",
            "completed",
        ],
        "app.OperandUncaught" => &["before", "after"],
        _ => return Err("unknown fixture entry".into()),
    };
    let observations: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: &entry,
            field,
        })
        .collect();
    let mut profile = ApplicationOperandExceptionProfile::default();
    let mut options = ApplicationOptions::default();
    let mut string_limits = crabspot_launcher::ApplicationStringLimits::default();
    let mut route = "profile";
    let mut values = Vec::new();
    let mut guest = false;
    for argument in args {
        if guest {
            values.push(
                argument
                    .into_string()
                    .map_err(|_| "invalid guest UTF-8 transport")?
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            );
            continue;
        }
        let option = argument
            .into_string()
            .map_err(|_| "invalid fixture option")?;
        match option.as_str() {
            "--" => guest = true,
            "strict" => profile.mode = OperandExceptionMode::RequireOperandExceptions,
            "disabled" => profile.mode = OperandExceptionMode::Disabled,
            "old" => route = "old",
            "ordinary" => route = "ordinary",
            _ => {
                let (key, text) = option.split_once('=').ok_or("invalid limit option")?;
                let value: u64 = text.parse()?;
                match key {
                    "vm" => profile.throwables.vm_exceptions.max_materializations = value,
                    "snapshots" => profile.throwables.traces.max_snapshots = value,
                    "trace-frames" => profile.throwables.traces.max_frames = value,
                    "depth" => profile.throwables.traces.max_frames_per_snapshot = value,
                    "trace-work" => profile.throwables.traces.max_work_units = value,
                    "discovery" => profile.discovery.max_work_units = value,
                    "diag-work" => profile.diagnostics.max_work_units = value,
                    "units" => profile.diagnostics.max_message_units = value,
                    "total-units" => profile.diagnostics.max_total_message_units = value,
                    "temporary" => profile.diagnostics.max_temporary_bytes = value,
                    "string-work" => string_limits.strings.max_work_units = value,
                    "string-objects" => string_limits.strings.max_objects = value,
                    "string-units" => string_limits.strings.max_retained_units = value,
                    "arrays" => options.heap.max_arrays = usize::try_from(value)?,
                    "payload" => options.heap.max_payload_bytes = usize::try_from(value)?,
                    "array-length" => options.heap.max_array_length = usize::try_from(value)?,
                    "instances" => options.instance.max_instances = usize::try_from(value)?,
                    "field" => options.field.max_lookup_steps = usize::try_from(value)?,
                    "fuel" => options.execution.instruction_budget = value,
                    _ => return Err("unknown limit option".into()),
                }
            }
        }
    }
    if !guest {
        return Err("missing -- guest delimiter".into());
    }
    let arguments = values.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let dirs = ApplicationDirectories {
        platform: &platform,
        classes: &classes,
    };
    let invocation = ApplicationInvocation {
        arguments: &arguments,
        strings: string_limits,
        ..Default::default()
    };
    let result = match route {
        "old" => run_directory_application_with_throwables(
            dirs,
            &entry,
            &observations,
            options,
            invocation,
            profile.throwables,
        ),
        "ordinary" => run_directory_application_with_arguments(
            dirs,
            &entry,
            &observations,
            options,
            invocation,
        ),
        _ => run_directory_application_with_operand_exceptions(
            dirs,
            &entry,
            &observations,
            options,
            invocation,
            profile,
        ),
    };
    let run = match result {
        Err(error) => {
            let why = match &error.kind {
                ApplicationErrorKind::UnsupportedPlatformReference => {
                    "UnsupportedPlatformReference".into()
                }
                ApplicationErrorKind::Throwable(_)
                | ApplicationErrorKind::OperandException(_)
                | ApplicationErrorKind::Setup { .. } => cause(error.as_ref())?,
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
                _ => cause(error.as_ref())?,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_operand_and_heap_limits_keep_distinct_typed_labels() {
        let diagnostic = OperandExceptionError {
            kind: OperandExceptionErrorKind::Limit {
                resource: crabspot_runtime::OperandExceptionResource::DiagnosticWork,
                limit: 39,
                required: 40,
            },
            discovery_usage: Default::default(),
        };
        assert_eq!(cause(&diagnostic).unwrap(), "DiagnosticWork 39/40");
        let heap = HeapError::ResourceLimit {
            resource: crabspot_runtime::HeapResource::Arrays,
            limit: 0,
            required: 1,
        };
        assert_eq!(cause(&heap).unwrap(), "HeapArrays 0/1");
        let nested = OperandExceptionError {
            kind: OperandExceptionErrorKind::Heap(Box::new(heap)),
            discovery_usage: Default::default(),
        };
        assert_eq!(cause(&nested).unwrap(), "HeapArrays 0/1");
    }
}
