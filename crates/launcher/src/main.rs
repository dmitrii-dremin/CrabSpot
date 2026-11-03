use crabspot_classfile::{Attribute, AttributeBody, ClassFile, ParseOptions, parse_with_options};
use std::{
    env,
    fs::File,
    io::{self, Read},
    path::PathBuf,
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("crabspot: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("inspect")) {
        return Err(usage());
    }
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
