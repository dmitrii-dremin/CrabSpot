//! Observe the monitor application fixtures through the same public entry used by the CLI.
use crabspot_interpreter::{ExecutionErrorKind, MonitorError};
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, MonitorLimits, StaticIntObservation,
    run_directory_application, run_directory_application_with_monitor_limits,
};
use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform directory")?);
    let classes = PathBuf::from(args.next().ok_or("missing application directory")?);
    let main = args
        .next()
        .and_then(|s| s.into_string().ok())
        .ok_or("missing main class")?;
    let steps = match args.next() {
        Some(value) => Some(
            value
                .into_string()
                .map_err(|_| "invalid step limit")?
                .parse::<u64>()?,
        ),
        None => None,
    };
    if args.next().is_some() {
        return Err("expected at most one monitor step limit".into());
    }
    let (owner, fields): (&str, &[&str]) = match main.as_str() {
        "locks.RawEntry" => ("locks.RawEntry", &["phase"]),
        "locks.Entry" | "locks.Null" => ("locks.Marks", &["entries", "calls", "completed", "sum"]),
        _ => return Err("unknown monitor fixture".into()),
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
    let run = match steps {
        None => run_directory_application(directories, &main, &queries, Default::default()),
        Some(max_steps) => run_directory_application_with_monitor_limits(
            directories,
            &main,
            &queries,
            Default::default(),
            MonitorLimits {
                max_steps,
                ..Default::default()
            },
        ),
    }?;
    if run.usage.argument_arrays != 1 || run.usage.engine_evaluations != 1 {
        return Err("monitor case did not use exactly one application invocation".into());
    }
    let completion = match run.completion {
        ApplicationCompletion::Returned => "returned".to_owned(),
        ApplicationCompletion::ExecutionFailed { phase, error } => {
            let cause = match &error.kind {
                ExecutionErrorKind::UnsupportedVmException(kind) => format!("{kind:?}"),
                ExecutionErrorKind::Monitor(MonitorError::Limit {
                    resource,
                    limit,
                    required,
                }) => {
                    format!("{resource:?} {limit}/{required}")
                }
                _ => return Err(error),
            };
            let method = error.method.as_ref().ok_or("missing failing method")?;
            let pc = error
                .offset
                .map_or_else(|| "root".to_owned(), |pc| pc.to_string());
            let caller = match &error.call_site {
                Some(site) => format!(" via {}@{}", site.method.name.escaped(), site.offset),
                None => String::new(),
            };
            format!("{phase:?} {cause} {}@{pc}{caller}", method.name.escaped())
        }
        ApplicationCompletion::SuppliedUncaught { class_name, .. } => {
            return Err(format!("unexpected supplied throwable {}", class_name.escaped()).into());
        }
    };
    let fields = run
        .static_ints
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>();
    println!("{completion} {}", fields.join("/"));
    Ok(())
}
