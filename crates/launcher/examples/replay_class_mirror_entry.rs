//! Observe static fields and the precise failure stage through the public directory launcher.
use crabspot_interpreter::{ExecutionErrorKind, MonitorError};
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationErrorKind, ClassMirrorLimits,
    MonitorLimits, StaticIntObservation, run_directory_application,
    run_directory_application_with_mirror_limits,
};
use crabspot_runtime::{
    ClassMirrorError, ConstructionAccessError, FieldAccessError, InstanceError, MethodAccessError,
};
use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform directory")?);
    let classes = PathBuf::from(args.next().ok_or("missing application directory")?);
    let main = args
        .next()
        .and_then(|s| s.into_string().ok())
        .ok_or("missing main")?;
    let (owner, fields): (&str, &[&str]) = match main.as_str() {
        "mirrors.Entry" => ("mirrors.Marks", &["flags", "initializers", "called"]),
        "store.Entry" => ("store.State", &["flags", "initialized"]),
        "simple.Plain" => ("simple.Plain", &["flag"]),
        "probe.Entry" => ("probe.Entry", &["phase"]),
        _ => return Err("unknown class-object fixture".into()),
    };
    let queries = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: owner,
            field,
        })
        .collect::<Vec<_>>();
    let directories = ApplicationDirectories {
        platform: &platform,
        classes: &classes,
    };
    let limits = args
        .map(|value| -> Result<u64, Box<dyn Error>> {
            Ok(value.into_string().map_err(|_| "invalid limit")?.parse()?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = match limits.as_slice() {
        [] => run_directory_application(directories, &main, &queries, Default::default()),
        &[
            max_mirrors,
            max_class_sites,
            max_active_monitors,
            max_recursion,
        ] => run_directory_application_with_mirror_limits(
            directories,
            &main,
            &queries,
            Default::default(),
            MonitorLimits {
                max_active_monitors,
                max_recursion,
                ..Default::default()
            },
            ClassMirrorLimits {
                max_mirrors,
                max_class_sites,
            },
        ),
        _ => {
            return Err(
                "supply mirror, class-site, active-monitor and recursion limits together".into(),
            );
        }
    };
    let run = match result {
        Ok(run) => run,
        Err(error) => {
            if error.usage.argument_arrays != 0 || error.usage.engine_evaluations != 0 {
                return Err("pre-execution failure consumed execution authority".into());
            }
            let kind = match error.kind {
                ApplicationErrorKind::Verification(_) => "Verification",
                ApplicationErrorKind::MissingApplicationDependency => {
                    "MissingApplicationDependency"
                }
                ApplicationErrorKind::UnsupportedPlatformReference => {
                    "UnsupportedPlatformReference"
                }
                _ => return Err(error),
            };
            let site = error
                .cp_index
                .map_or_else(|| "-".to_owned(), |i| i.to_string());
            println!("image {:?} {kind} cp={site}", error.stage);
            return Ok(());
        }
    };
    if run.usage.argument_arrays != 1
        || run.usage.engine_evaluations != 1
        || run.usage.proved_classes != run.usage.preparation_witnesses
        || run.usage.proved_classes != run.usage.admitted_executable_classes
    {
        return Err("inconsistent application proof or execution counts".into());
    }
    let result = match run.completion {
        ApplicationCompletion::Returned => "returned".to_owned(),
        ApplicationCompletion::ExecutionFailed { phase, error } => {
            let cause = match &error.kind {
                ExecutionErrorKind::UnsupportedVmException(kind) => format!("{kind:?}"),
                ExecutionErrorKind::ClassMirror(e) => match e.as_ref() {
                    ClassMirrorError::Limit {
                        resource,
                        limit,
                        required,
                    } => format!("{resource:?} {limit}/{required}"),
                    _ => return Err(error),
                },
                ExecutionErrorKind::Monitor(MonitorError::Limit {
                    resource,
                    limit,
                    required,
                }) => format!("{resource:?} {limit}/{required}"),
                ExecutionErrorKind::ConstructionAccess(e)
                    if matches!(e.as_ref(), ConstructionAccessError::Instance(source)
                        if matches!(source.as_ref(), InstanceError::ReservedClassObject { .. })) =>
                {
                    "ReservedClassObject".to_owned()
                }
                ExecutionErrorKind::UnsupportedMethod(
                    "native methods require runtime services",
                ) => "UnboundNative".to_owned(),
                ExecutionErrorKind::MethodAccess(source)
                    if matches!(source.as_ref(), MethodAccessError::Context(FieldAccessError::MemberResolution(member))
                        if matches!(&member.kind, crabspot_runtime::MemberResolutionErrorKind::Field(field)
                            if matches!(field.as_ref(), FieldAccessError::Instance(InstanceError::LayoutMissing { .. }))))
                        && run.usage.string_equality.natives_registered
                        && run.usage.string_equality.assertion_queries == 0 =>
                {
                    "LayoutMissing".to_owned()
                }
                ExecutionErrorKind::MethodAccess(e)
                    if matches!(
                        e.as_ref(),
                        MethodAccessError::Context(FieldAccessError::Instance(
                            InstanceError::OpaqueClassObject { .. }
                        ))
                    ) =>
                {
                    "OpaqueClassObject".to_owned()
                }
                _ => return Err(error),
            };
            let method = error.method.as_ref().ok_or("missing failure identity")?;
            let initializing = if let Some(class) = error.initialization_class {
                if Some(class) != method.owner {
                    return Err("unexpected initializer failure outside its declaring class".into());
                }
                "initializing "
            } else {
                ""
            };
            let offset = match error.offset {
                Some(pc) => pc.to_string(),
                None if cause == "UnboundNative" => "entry".to_owned(),
                None => return Err("missing failure bytecode offset".into()),
            };
            let caller = error.call_site.as_ref().map_or_else(String::new, |site| {
                format!(" via {}@{}", site.method.name.escaped(), site.offset)
            });
            format!(
                "{phase:?} {initializing}{cause} {}@{offset}{caller}",
                method.name.escaped()
            )
        }
        ApplicationCompletion::Uncaught {
            phase,
            class_name,
            location,
        } => {
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
        }
        ApplicationCompletion::SuppliedUncaught { class_name, .. } => {
            return Err(format!("unexpected supplied Throwable {}", class_name.escaped()).into());
        }
    };
    let fields = run
        .static_ints
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join("/");
    println!(
        "{result} fields={fields} proved={}",
        run.usage.proved_classes
    );
    Ok(())
}
