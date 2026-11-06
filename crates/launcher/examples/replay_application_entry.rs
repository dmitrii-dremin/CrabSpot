//! Read effects of the original Java directory fixtures through the public launcher API.
use crabspot_interpreter::ExecutionErrorKind;
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, StaticIntObservation, run_directory_application,
};
use std::{env, error::Error, path::PathBuf};

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let platform = PathBuf::from(args.next().ok_or("missing platform directory")?);
    let classes = PathBuf::from(args.next().ok_or("missing application directory")?);
    let entry = args
        .next()
        .and_then(|s| s.into_string().ok())
        .ok_or("missing binary entry name")?;
    if args.next().is_some() {
        return Err("expected platform directory, application directory and entry name".into());
    }
    let (owner, fields): (&str, &[&str]) = if entry.starts_with("named.") {
        ("named.Trace", &["marks", "body"])
    } else {
        (
            "app.Marks",
            &["entered", "completed", "total", "initialized", "slots"],
        )
    };
    let queries: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: owner,
            field,
        })
        .collect();
    let result = run_directory_application(
        ApplicationDirectories {
            platform: &platform,
            classes: &classes,
        },
        &entry,
        &queries,
        Default::default(),
    );
    let run = match result {
        Ok(run) => run,
        Err(error) => {
            if error.usage.engine_evaluations != 0 || error.usage.argument_arrays != 0 {
                return Err(format!("preflight failure has execution effects: {error:?}").into());
            }
            println!("image {:?}", error.stage);
            eprintln!("{error}");
            return Ok(());
        }
    };
    if run.usage.engine_evaluations != 1
        || run.usage.argument_arrays != 1
        || run.usage.admitted_executable_classes == 0
        || run.usage.admitted_executable_classes != run.usage.proved_classes
        || run.usage.admitted_executable_classes != run.usage.preparation_witnesses
    {
        return Err(format!("incomplete verified application run: {:?}", run.usage).into());
    }
    let outcome = match run.completion {
        ApplicationCompletion::Returned => "returned".to_owned(),
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
        ApplicationCompletion::SuppliedUncaught { phase, class_name } => {
            format!("{phase:?} uncaught {}", class_name.escaped())
        }
        ApplicationCompletion::ExecutionFailed { phase, error } => match &error.kind {
            ExecutionErrorKind::UnsupportedVmException(kind) => format!("{phase:?} {kind:?}"),
            other => {
                return Err(format!("unexpected engine diagnostic at {phase:?}: {other:?}").into());
            }
        },
    };
    let values = run
        .static_ints
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join("/");
    println!("{outcome} {values}");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    run()
}
