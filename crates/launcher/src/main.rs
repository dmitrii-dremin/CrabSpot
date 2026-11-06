mod native_arguments;
mod run_arguments;
use crabspot_classfile::{Attribute, AttributeBody, ClassFile, ParseOptions, parse_with_options};
use crabspot_launcher::{
    ApplicationCompletion, ApplicationDirectories, ApplicationInvocation, ApplicationOptions,
    ApplicationStringLimits, run_directory_application_with_arguments,
};
use std::{
    env,
    fs::File,
    io::{self, Read},
    path::PathBuf,
    process::ExitCode,
};

enum CliError {
    Usage(String),
    Failure(String),
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("crabspot: {message}");
            ExitCode::from(2)
        }
        Err(CliError::Failure(message)) => {
            eprintln!("crabspot: {message}");
            ExitCode::FAILURE
        }
    }
}
fn full_usage() -> String {
    format!(
        "{}\n       crabspot run --platform DIR --classes DIR [--field-lookup-steps N] BINARY.MAIN [-- ARG ...]",
        usage()
    )
}
fn run() -> Result<(), CliError> {
    let mut arguments = env::args_os().skip(1);
    match arguments.next().as_deref() {
        Some(command) if command == "inspect" => inspect_run(arguments).map_err(CliError::Failure),
        Some(command) if command == "run" => application_run(arguments),
        _ => Err(CliError::Usage(full_usage())),
    }
}
fn application_run(
    mut arguments: impl ExactSizeIterator<Item = std::ffi::OsString>,
) -> Result<(), CliError> {
    let parsed = run_arguments::parse(&mut arguments).map_err(|_| CliError::Usage(full_usage()))?;
    let entry = parsed
        .entry
        .into_string()
        .map_err(|_| CliError::Failure("Input: binary entry name is not valid UTF-8".into()))?;
    let mut options = ApplicationOptions::default();
    if let Some(limit) = parsed.field_lookup_steps {
        options.field.max_lookup_steps = limit;
    }
    let limits = ApplicationStringLimits::default();
    let values = native_arguments::decode(arguments, limits)
        .map_err(|e| CliError::Failure(format!("Input: {e}")))?;
    let mut borrowed = Vec::new();
    borrowed
        .try_reserve_exact(values.len())
        .map_err(|_| CliError::Failure("Input: argument references allocation failed".into()))?;
    borrowed.extend(values.iter().map(Vec::as_slice));
    let platform = parsed.platform;
    let classes = parsed.classes;
    let run = run_directory_application_with_arguments(
        ApplicationDirectories {
            platform: &platform,
            classes: &classes,
        },
        &entry,
        &[],
        options,
        ApplicationInvocation {
            arguments: &borrowed,
            strings: limits,
            ..Default::default()
        },
    )
    .map_err(|error| CliError::Failure(error.to_string()))?;
    match run.completion {
        ApplicationCompletion::Returned => Ok(()),
        ApplicationCompletion::ExecutionFailed { phase, error } => {
            Err(CliError::Failure(format!("{phase:?}: {error}")))
        }
        ApplicationCompletion::Uncaught {
            phase,
            class_name,
            location,
        } => {
            let at = location.map_or_else(String::new, |head| {
                format!(
                    " at {}.{}{}@{}",
                    head.class_name.escaped(),
                    head.method_name.escaped(),
                    head.descriptor.escaped(),
                    head.offset
                )
            });
            Err(CliError::Failure(format!(
                "{phase:?}: uncaught {}{at}",
                class_name.escaped()
            )))
        }
        ApplicationCompletion::SuppliedUncaught { phase, class_name } => Err(CliError::Failure(
            format!("{phase:?}: uncaught {}", class_name.escaped()),
        )),
    }
}
fn inspect_run(arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    let mut options = ParseOptions::default();
    let mut path = None;
    for argument in arguments {
        if argument == "--enable-preview" {
            options.allow_preview = true;
        } else if path.is_none() {
            path = Some(PathBuf::from(argument));
        } else {
            return Err(usage());
        }
    }
    let path = path.ok_or_else(usage)?;
    let file =
        File::open(&path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let limit = options.max_class_bytes;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let class = parse_with_options(&bytes, options).map_err(|error| error.to_string())?;
    let metadata = inspect(&class)?;
    let mut stdout = io::stdout().lock();
    std::io::Write::write_all(&mut stdout, metadata.as_bytes())
        .map_err(|error| format!("cannot write inspection: {error}"))?;
    Ok(())
}

fn usage() -> String {
    "usage: crabspot inspect [--enable-preview] <class-file>".to_owned()
}

fn inspect(class: &ClassFile) -> Result<String, String> {
    let pool = &class.constant_pool;
    let mut output = format!(
        "class: {}\nversion: {}.{}{}\naccess_flags: 0x{:04x}\nconstant_pool_count: {}\n",
        class.name().map_err(|e| e.to_string())?.escaped(),
        class.version.major,
        class.version.minor,
        if class.version.is_preview() {
            " (preview)"
        } else {
            ""
        },
        class.access_flags,
        pool.entries().len()
    );
    if class.super_class == 0 {
        output.push_str("super: <none>\n");
    } else {
        output.push_str(&format!(
            "super: {}\n",
            pool.class_name(class.super_class)
                .map_err(|e| e.to_string())?
                .escaped()
        ));
    }
    output.push_str(&format!("interfaces: {}\n", class.interfaces.len()));
    for index in &class.interfaces {
        output.push_str(&format!(
            "  {}\n",
            pool.class_name(*index)
                .map_err(|e| e.to_string())?
                .escaped()
        ));
    }
    for (label, members) in [("fields", &class.fields), ("methods", &class.methods)] {
        output.push_str(&format!("{label}: {}\n", members.len()));
        for member in members {
            output.push_str(&format!(
                "  {} {} flags=0x{:04x}\n",
                pool.utf8(member.name_index)
                    .map_err(|e| e.to_string())?
                    .escaped(),
                pool.utf8(member.descriptor_index)
                    .map_err(|e| e.to_string())?
                    .escaped(),
                member.access_flags
            ));
        }
    }
    let mut opaque_standard = 0;
    for attribute in &class.attributes {
        count_opaque(attribute, &mut opaque_standard);
    }
    for member in class.fields.iter().chain(&class.methods) {
        for attribute in &member.attributes {
            count_opaque(attribute, &mut opaque_standard);
        }
    }
    output.push_str(&format!("attributes: {}\nopaque_standard_attributes: {opaque_standard}\ninspection only; bytecode has not been verified or executed\n", class.attributes.len()));
    Ok(output)
}

fn count_opaque(attribute: &Attribute, count: &mut usize) {
    match &attribute.body {
        AttributeBody::Opaque { standard: true, .. } => *count += 1,
        AttributeBody::Code(code) => {
            for attribute in &code.attributes {
                count_opaque(attribute, count);
            }
        }
        AttributeBody::Record(components) => {
            for component in components {
                for attribute in &component.attributes {
                    count_opaque(attribute, count);
                }
            }
        }
        _ => {}
    }
}
