use crate::{
    Attribute, AttributeBody, ClassFile, ClassVersion, Constant, ConstantPool, ErrorKind, Member,
    ParseError, TypeAnnotation, TypeAnnotationTarget,
    annotation::AnnotationBudget,
    attribute::{Location, parse_attributes},
    constant_pool::parse_pool,
    descriptor,
    reader::Reader,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseOptions {
    pub allow_preview: bool,
    /// Reject oversized input before allocating the parsed model.
    pub max_class_bytes: usize,
    /// Annotation/array nesting, with roots at depth one. Requests above 64
    /// are clamped to 64 to bound host-stack usage independently of policy.
    pub max_annotation_depth: usize,
    /// Shared whole-file budget counting annotation and element-value nodes.
    pub max_annotation_values: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            allow_preview: false,
            max_class_bytes: 16 * 1024 * 1024,
            max_annotation_depth: 64,
            max_annotation_values: 1_000_000,
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<ClassFile, ParseError> {
    parse_with_options(bytes, ParseOptions::default())
}

pub fn parse_with_options(bytes: &[u8], options: ParseOptions) -> Result<ClassFile, ParseError> {
    if bytes.len() > options.max_class_bytes {
        return Err(ParseError::new(
            0,
            "class file",
            ErrorKind::ResourceLimit {
                resource: "class-file bytes",
                limit: options.max_class_bytes,
                actual: bytes.len(),
            },
        ));
    }
    let mut reader = Reader::new(bytes, 0, "class file");
    let mut annotation_budget = AnnotationBudget::new(options);
    let magic = reader.u4()?;
    if magic != 0xcafebabe {
        return Err(ParseError::new(0, "header", ErrorKind::InvalidMagic(magic)));
    }
    let minor = reader.u2()?;
    let major = reader.u2()?;
    if !(45..=65).contains(&major)
        || (major >= 56 && minor != 0 && minor != u16::MAX)
        || ((56..65).contains(&major) && minor == u16::MAX)
    {
        return Err(ParseError::new(
            4,
            "version",
            ErrorKind::UnsupportedVersion { major, minor },
        ));
    }
    if major == 65 && minor == u16::MAX && !options.allow_preview {
        return Err(ParseError::new(4, "version", ErrorKind::PreviewDisabled));
    }
    let constant_pool = parse_pool(&mut reader, major)?;
    let access_flags = reader.u2()?;
    let this_class = reader.u2()?;
    let super_class = reader.u2()?;
    let this_name = constant_pool.class_name(this_class)?;
    if !descriptor::valid_class_name(this_name.units()) {
        return Err(reader.invalid("this_class must name a non-array class"));
    }
    if super_class != 0 {
        non_array_class(&constant_pool, super_class)?;
    }
    if access_flags & 0x8000 != 0 {
        if major < 53
            || access_flags != 0x8000
            || !this_name.equals_ascii("module-info")
            || super_class != 0
        {
            return Err(reader.invalid("invalid module-info class header"));
        }
    } else if this_name.equals_ascii("java/lang/Object") {
        if super_class != 0 {
            return Err(reader.invalid("java/lang/Object must have super_class zero"));
        }
    } else if super_class == 0 {
        return Err(
            reader.invalid("only java/lang/Object or module-info may have super_class zero")
        );
    }
    if access_flags & 0x0410 == 0x0410 {
        return Err(reader.invalid("a class cannot be both final and abstract"));
    }
    if access_flags & 0x0200 != 0 {
        if access_flags & 0x0400 == 0 || access_flags & 0x4030 != 0 {
            return Err(reader.invalid("invalid interface access flags"));
        }
        if super_class == 0
            || !constant_pool
                .class_name(super_class)?
                .equals_ascii("java/lang/Object")
        {
            return Err(reader.invalid("an interface must have java/lang/Object as superclass"));
        }
    } else if access_flags & 0x2000 != 0 {
        return Err(reader.invalid("an annotation class must also be an interface"));
    }
    let mut interfaces = Vec::new();
    let mut interface_names = BTreeSet::new();
    for _ in 0..reader.u2()? {
        let index = reader.u2()?;
        non_array_class(&constant_pool, index)?;
        if !interface_names.insert(constant_pool.class_name(index)?.units()) {
            return Err(reader.invalid("duplicate interface"));
        }
        interfaces.push(index);
    }
    let fields = members(
        &mut reader,
        &constant_pool,
        major,
        access_flags,
        false,
        &mut annotation_budget,
    )?;
    let methods = members(
        &mut reader,
        &constant_pool,
        major,
        access_flags,
        true,
        &mut annotation_budget,
    )?;
    let attributes = parse_attributes(
        &mut reader,
        &constant_pool,
        major,
        Location::Class,
        &mut annotation_budget,
    )?;
    reader.finish()?;
    for annotation in member_type_annotations(&attributes) {
        if let TypeAnnotationTarget::Supertype(index) = annotation.target {
            let valid = if index == u16::MAX {
                super_class != 0 && access_flags & 0x0200 == 0
            } else {
                usize::from(index) < interfaces.len()
            };
            if !valid {
                return Err(reader
                    .invalid("type-annotation supertype index is outside the enclosing class"));
            }
        }
    }
    if access_flags & 0x8000 != 0 {
        if !interfaces.is_empty() || !fields.is_empty() || !methods.is_empty() {
            return Err(reader.invalid("module-info cannot declare interfaces, fields or methods"));
        }
        if !attributes.iter().any(|attribute| {
            constant_pool
                .utf8(attribute.name_index)
                .is_ok_and(|name| name.equals_ascii("Module"))
        }) {
            return Err(reader.invalid("module-info requires a Module attribute"));
        }
        for attribute in &attributes {
            if matches!(
                attribute.body,
                AttributeBody::Opaque {
                    standard: false,
                    ..
                }
            ) {
                continue;
            }
            let name = constant_pool.utf8(attribute.name_index)?;
            if !matches!(
                name.to_string_lossy().as_str(),
                "Module"
                    | "ModulePackages"
                    | "ModuleMainClass"
                    | "InnerClasses"
                    | "SourceFile"
                    | "SourceDebugExtension"
                    | "RuntimeVisibleAnnotations"
                    | "RuntimeInvisibleAnnotations"
            ) {
                return Err(reader.invalid("standard attribute is not permitted on module-info"));
            }
        }
        let module = attributes
            .iter()
            .find_map(|attribute| match &attribute.body {
                AttributeBody::Module(module) => Some(module),
                _ => None,
            });
        let packages = attributes
            .iter()
            .find_map(|attribute| match &attribute.body {
                AttributeBody::ModulePackages(packages) => Some(packages),
                _ => None,
            });
        if let (Some(module), Some(packages)) = (module, packages) {
            crate::module::validate_module_packages(
                module,
                &constant_pool,
                packages,
                reader.offset(),
            )?;
        }
    } else {
        if constant_pool
            .entries()
            .iter()
            .any(|entry| matches!(entry, Constant::Module { .. } | Constant::Package { .. }))
        {
            return Err(reader.invalid("Module/Package constants require ACC_MODULE"));
        }
        if major >= 53
            && attributes.iter().any(|attribute| {
                constant_pool.utf8(attribute.name_index).is_ok_and(|name| {
                    matches!(
                        name.to_string_lossy().as_str(),
                        "Module" | "ModulePackages" | "ModuleMainClass"
                    )
                })
            })
        {
            return Err(reader.invalid("module attributes require ACC_MODULE"));
        }
    }
    let has_nest_host = attributes
        .iter()
        .any(|attribute| matches!(attribute.body, AttributeBody::NestHost(_)));
    let has_nest_members = attributes
        .iter()
        .any(|attribute| matches!(attribute.body, AttributeBody::NestMembers(_)));
    if has_nest_host && has_nest_members {
        return Err(reader.invalid("NestHost and NestMembers cannot both be present"));
    }
    if access_flags & 0x0010 != 0
        && attributes
            .iter()
            .any(|attribute| matches!(attribute.body, AttributeBody::PermittedSubclasses(_)))
    {
        return Err(reader.invalid("a final class cannot have PermittedSubclasses"));
    }
    let bootstrap_count = attributes
        .iter()
        .find_map(|attribute| match &attribute.body {
            AttributeBody::BootstrapMethods(methods) => Some(methods.len()),
            _ => None,
        })
        .unwrap_or(0);
    for constant in constant_pool.entries() {
        if let Constant::Dynamic {
            bootstrap_method_attr_index,
            ..
        }
        | Constant::InvokeDynamic {
            bootstrap_method_attr_index,
            ..
        } = constant
        {
            if usize::from(*bootstrap_method_attr_index) >= bootstrap_count {
                return Err(
                    reader.invalid("dynamic constant has an out-of-range BootstrapMethods index")
                );
            }
        }
    }
    Ok(ClassFile {
        version: ClassVersion { major, minor },
        constant_pool,
        access_flags,
        this_class,
        super_class,
        interfaces,
        fields,
        methods,
        attributes,
    })
}

fn non_array_class(pool: &ConstantPool, index: u16) -> Result<(), ParseError> {
    if descriptor::valid_class_name(pool.class_name(index)?.units()) {
        Ok(())
    } else {
        Err(pool.index_error(index, "a non-array Class"))
    }
}

fn member_type_annotations(attributes: &[Attribute]) -> impl Iterator<Item = &TypeAnnotation> {
    attributes
        .iter()
        .filter_map(|attribute| match &attribute.body {
            AttributeBody::RuntimeVisibleTypeAnnotations(annotations)
            | AttributeBody::RuntimeInvisibleTypeAnnotations(annotations) => {
                Some(annotations.as_slice())
            }
            _ => None,
        })
        .flatten()
}

fn members(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    major: u16,
    class_flags: u16,
    method: bool,
    annotation_budget: &mut AnnotationBudget,
) -> Result<Vec<Member>, ParseError> {
    let mut members = Vec::new();
    let mut identities = BTreeSet::new();
    let mut seen_class_initializer = false;
    for _ in 0..reader.u2()? {
        let offset = reader.offset();
        let access_flags = reader.u2()?;
        let name_index = reader.u2()?;
        let descriptor_index = reader.u2()?;
        let name = pool.utf8(name_index)?;
        let desc = pool.utf8(descriptor_index)?;
        if !identities.insert((name.units(), desc.units())) {
            return Err(reader.invalid("duplicate member name and descriptor"));
        }
        let special = method && (name.equals_ascii("<init>") || name.equals_ascii("<clinit>"));
        let class_initializer = method && name.equals_ascii("<clinit>");
        if class_initializer {
            if seen_class_initializer {
                return Err(reader.invalid("a class may have only one class initializer"));
            }
            seen_class_initializer = true;
        }
        if class_flags & 0x0200 != 0 && !class_initializer {
            if method {
                if name.equals_ascii("<init>") || access_flags & 0x0134 != 0 {
                    return Err(reader.invalid("invalid interface method name or access flags"));
                }
                if major < 52 {
                    if access_flags & 0x0403 != 0x0401 {
                        return Err(reader.invalid(
                            "interface methods before version 52 must be public and abstract",
                        ));
                    }
                } else if (access_flags & 3).count_ones() != 1 {
                    return Err(reader
                        .invalid("interface methods must be exactly one of public or private"));
                }
            } else if access_flags & 0x0019 != 0x0019 || access_flags & 0x40c6 != 0 {
                return Err(reader.invalid("interface fields must be public, static and final"));
            }
        }
        if !special && !descriptor::valid_unqualified_name(name.units(), method) {
            return Err(reader.invalid("invalid member name"));
        }
        let visibility = access_flags & 7;
        if !class_initializer && visibility.count_ones() > 1 {
            return Err(reader.invalid("conflicting member visibility flags"));
        }
        let method_descriptor = if method {
            let descriptor = descriptor::parse_method_descriptor(desc.units()).map_err(|_| {
                ParseError::new(
                    offset + 4,
                    "method descriptor",
                    ErrorKind::InvalidDescriptor,
                )
            })?;
            let receiver = if !class_initializer && access_flags & 8 == 0 {
                1
            } else {
                0
            };
            if descriptor.parameter_slots + receiver > 255 {
                return Err(reader.invalid("method parameter slots including receiver exceed 255"));
            }
            if name.equals_ascii("<init>")
                && (descriptor.return_type.is_some() || access_flags & 0x0578 != 0)
            {
                return Err(reader.invalid("invalid constructor descriptor or flags"));
            }
            if name.equals_ascii("<clinit>")
                && (descriptor.return_type.is_some()
                    || (major >= 51 && (!desc.equals_ascii("()V") || access_flags & 8 == 0)))
            {
                return Err(reader.invalid("invalid class initializer descriptor or flags"));
            }
            let abstract_conflicts = 0x013a
                | if (46..=60).contains(&major) {
                    0x0800
                } else {
                    0
                };
            if !class_initializer
                && access_flags & 0x0400 != 0
                && access_flags & abstract_conflicts != 0
            {
                return Err(reader.invalid("abstract method has incompatible access flags"));
            }
            Some(descriptor)
        } else {
            descriptor::parse_field_descriptor(desc.units()).map_err(|_| {
                ParseError::new(offset + 4, "field descriptor", ErrorKind::InvalidDescriptor)
            })?;
            if access_flags & 0x0050 == 0x0050 {
                return Err(reader.invalid("field cannot be both final and volatile"));
            }
            None
        };
        let location = if method {
            Location::Method
        } else {
            Location::Field
        };
        let attributes = parse_attributes(reader, pool, major, location, annotation_budget)?;
        if let Some(descriptor) = method_descriptor {
            let exceptions = attributes
                .iter()
                .find_map(|attribute| match &attribute.body {
                    AttributeBody::Exceptions(exceptions) => Some(exceptions.len()),
                    _ => None,
                })
                .unwrap_or(0);
            for annotation in member_type_annotations(&attributes) {
                let valid = match annotation.target {
                    TypeAnnotationTarget::MethodFormalParameter(index) => {
                        usize::from(index) < descriptor.parameters.len()
                    }
                    TypeAnnotationTarget::Throws(index) => usize::from(index) < exceptions,
                    TypeAnnotationTarget::MethodReceiver => {
                        access_flags & 8 == 0 && !class_initializer
                    }
                    _ => true,
                };
                if !valid {
                    return Err(reader.invalid(
                        "type-annotation target is outside the enclosing method declaration",
                    ));
                }
            }
            if attributes
                .iter()
                .any(|attribute| matches!(attribute.body, AttributeBody::AnnotationDefault(_)))
                && (class_flags & 0x2000 == 0
                    || special
                    || !descriptor.parameters.is_empty()
                    || descriptor.return_type.is_none())
            {
                return Err(reader.invalid(
                    "AnnotationDefault requires an annotation-interface element declaration",
                ));
            }
            let code = attributes
                .iter()
                .find_map(|attribute| match &attribute.body {
                    AttributeBody::Code(code) => Some(code),
                    _ => None,
                });
            let requires_code = class_initializer || access_flags & 0x0500 == 0;
            if requires_code != code.is_some() {
                return Err(reader.invalid(
                    "concrete methods require Code; abstract/native methods must not have Code",
                ));
            }
            if let Some(code) = code {
                // Historical class initializers may have parameter descriptors,
                // but the VM invokes class initialization without arguments.
                let required = if class_initializer {
                    0
                } else {
                    descriptor.parameter_slots + if access_flags & 8 == 0 { 1 } else { 0 }
                };
                if code.max_locals < required {
                    return Err(reader.invalid("Code max_locals cannot hold the method parameters"));
                }
            }
        } else {
            for attribute in &attributes {
                if let AttributeBody::ConstantValue(index) = attribute.body {
                    let constant = pool.get(index)?;
                    let valid = match desc.units().first() {
                        Some(66 | 67 | 73 | 83 | 90) => matches!(constant, Constant::Integer(_)),
                        Some(74) => matches!(constant, Constant::Long(_)),
                        Some(70) => matches!(constant, Constant::Float(_)),
                        Some(68) => matches!(constant, Constant::Double(_)),
                        _ => {
                            desc.equals_ascii("Ljava/lang/String;")
                                && matches!(constant, Constant::String { .. })
                        }
                    };
                    if !valid {
                        return Err(reader
                            .invalid("ConstantValue type does not match the field descriptor"));
                    }
                }
            }
        }
        members.push(Member {
            offset,
            access_flags,
            name_index,
            descriptor_index,
            attributes,
        });
    }
    Ok(members)
}
