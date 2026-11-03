//! Annotation encodings (JVMS 4.7.16--4.7.22). Resolution and reflective value
//! compatibility are separate from these file-local structural checks.
use crate::{
    Constant, ConstantPool, ErrorKind, ParseError, ParseOptions,
    attribute::{CodeContext, Location},
    descriptor::{self, FieldType},
    reader::Reader,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Annotation {
    pub type_index: u16,
    pub elements: Vec<ElementValuePair>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElementValuePair {
    pub element_name_index: u16,
    pub value: ElementValue,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationConstantTag {
    Byte,
    Char,
    Double,
    Float,
    Int,
    Long,
    Short,
    Boolean,
    String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ElementValue {
    Constant {
        tag: AnnotationConstantTag,
        constant_index: u16,
    },
    Enum {
        type_name_index: u16,
        const_name_index: u16,
    },
    Class {
        class_info_index: u16,
    },
    Annotation(Box<Annotation>),
    Array(Vec<ElementValue>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeAnnotation {
    pub target: TypeAnnotationTarget,
    pub path: Vec<TypePathEntry>,
    pub annotation: Annotation,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeAnnotationTarget {
    ClassTypeParameter(u8),
    MethodTypeParameter(u8),
    Supertype(u16),
    ClassTypeParameterBound {
        type_parameter_index: u8,
        bound_index: u8,
    },
    MethodTypeParameterBound {
        type_parameter_index: u8,
        bound_index: u8,
    },
    Field,
    MethodReturn,
    MethodReceiver,
    MethodFormalParameter(u8),
    Throws(u16),
    LocalVariable(Vec<LocalVariableTarget>),
    ResourceVariable(Vec<LocalVariableTarget>),
    ExceptionParameter(u16),
    InstanceOf(u16),
    New(u16),
    ConstructorReference(u16),
    MethodReference(u16),
    Cast {
        offset: u16,
        type_argument_index: u8,
    },
    ConstructorInvocationTypeArgument {
        offset: u16,
        type_argument_index: u8,
    },
    MethodInvocationTypeArgument {
        offset: u16,
        type_argument_index: u8,
    },
    ConstructorReferenceTypeArgument {
        offset: u16,
        type_argument_index: u8,
    },
    MethodReferenceTypeArgument {
        offset: u16,
        type_argument_index: u8,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalVariableTarget {
    pub start_pc: u16,
    pub length: u16,
    pub index: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypePathKind {
    Array,
    InnerType,
    WildcardBound,
    TypeArgument,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypePathEntry {
    pub kind: TypePathKind,
    pub type_argument_index: u8,
}

pub(crate) struct AnnotationBudget {
    depth_limit: usize,
    value_limit: usize,
    used: usize,
}
impl AnnotationBudget {
    pub fn new(options: ParseOptions) -> Self {
        Self {
            depth_limit: options.max_annotation_depth.min(64),
            value_limit: options.max_annotation_values,
            used: 0,
        }
    }
    fn node(&mut self, reader: &Reader<'_>, depth: usize) -> Result<(), ParseError> {
        if depth > self.depth_limit {
            return Err(reader.error(ErrorKind::ResourceLimit {
                resource: "annotation nesting depth",
                limit: self.depth_limit,
                actual: depth,
            }));
        }
        if self.used >= self.value_limit {
            return Err(reader.error(ErrorKind::ResourceLimit {
                resource: "annotation and element-value nodes",
                limit: self.value_limit,
                actual: self.used.saturating_add(1),
            }));
        }
        self.used += 1;
        Ok(())
    }
}

fn object_descriptor(
    reader: &Reader<'_>,
    pool: &ConstantPool,
    index: u16,
) -> Result<(), ParseError> {
    if matches!(
        descriptor::parse_field_descriptor(pool.utf8(index)?.units()),
        Ok(FieldType::Object(_))
    ) {
        Ok(())
    } else {
        Err(reader.error(ErrorKind::InvalidDescriptor))
    }
}
fn name(
    reader: &Reader<'_>,
    pool: &ConstantPool,
    index: u16,
    method: bool,
) -> Result<(), ParseError> {
    if descriptor::valid_unqualified_name(pool.utf8(index)?.units(), method) {
        Ok(())
    } else {
        Err(reader.invalid("invalid annotation element or enum constant name"))
    }
}
fn annotation(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    budget: &mut AnnotationBudget,
    depth: usize,
) -> Result<Annotation, ParseError> {
    budget.node(reader, depth)?;
    let type_index = reader.u2()?;
    object_descriptor(reader, pool, type_index)?;
    let count = reader.u2()?;
    let mut elements = Vec::new();
    for _ in 0..count {
        let element_name_index = reader.u2()?;
        name(reader, pool, element_name_index, true)?;
        let value = element_value(reader, pool, budget, depth)?;
        elements.push(ElementValuePair {
            element_name_index,
            value,
        });
    }
    Ok(Annotation {
        type_index,
        elements,
    })
}

pub(crate) fn element_value(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    budget: &mut AnnotationBudget,
    depth: usize,
) -> Result<ElementValue, ParseError> {
    budget.node(reader, depth)?;
    let tag = reader.u1()?;
    let constant_tag = match tag {
        b'B' => Some(AnnotationConstantTag::Byte),
        b'C' => Some(AnnotationConstantTag::Char),
        b'D' => Some(AnnotationConstantTag::Double),
        b'F' => Some(AnnotationConstantTag::Float),
        b'I' => Some(AnnotationConstantTag::Int),
        b'J' => Some(AnnotationConstantTag::Long),
        b'S' => Some(AnnotationConstantTag::Short),
        b'Z' => Some(AnnotationConstantTag::Boolean),
        b's' => Some(AnnotationConstantTag::String),
        _ => None,
    };
    if let Some(tag) = constant_tag {
        let constant_index = reader.u2()?;
        pool.require(
            constant_index,
            "the constant type required by the annotation tag",
            |constant| match tag {
                AnnotationConstantTag::Byte
                | AnnotationConstantTag::Char
                | AnnotationConstantTag::Int
                | AnnotationConstantTag::Short
                | AnnotationConstantTag::Boolean => matches!(constant, Constant::Integer(_)),
                AnnotationConstantTag::Double => matches!(constant, Constant::Double(_)),
                AnnotationConstantTag::Float => matches!(constant, Constant::Float(_)),
                AnnotationConstantTag::Long => matches!(constant, Constant::Long(_)),
                AnnotationConstantTag::String => matches!(constant, Constant::Utf8(_)),
            },
        )?;
        return Ok(ElementValue::Constant {
            tag,
            constant_index,
        });
    }
    Ok(match tag {
        b'e' => {
            let type_name_index = reader.u2()?;
            let const_name_index = reader.u2()?;
            object_descriptor(reader, pool, type_name_index)?;
            name(reader, pool, const_name_index, false)?;
            ElementValue::Enum {
                type_name_index,
                const_name_index,
            }
        }
        b'c' => {
            let class_info_index = reader.u2()?;
            let descriptor = pool.utf8(class_info_index)?;
            if !descriptor.equals_ascii("V")
                && descriptor::parse_field_descriptor(descriptor.units()).is_err()
            {
                return Err(reader.error(ErrorKind::InvalidDescriptor));
            }
            ElementValue::Class { class_info_index }
        }
        b'@' => ElementValue::Annotation(Box::new(annotation(reader, pool, budget, depth + 1)?)),
        b'[' => {
            let count = reader.u2()?;
            let mut values = Vec::new();
            for _ in 0..count {
                values.push(element_value(reader, pool, budget, depth + 1)?);
            }
            ElementValue::Array(values)
        }
        _ => return Err(reader.invalid("invalid annotation element_value tag")),
    })
}
pub(crate) fn annotations(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    budget: &mut AnnotationBudget,
) -> Result<Vec<Annotation>, ParseError> {
    let count = reader.u2()?;
    let mut result = Vec::new();
    for _ in 0..count {
        result.push(annotation(reader, pool, budget, 1)?);
    }
    Ok(result)
}
pub(crate) fn parameter_annotations(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    budget: &mut AnnotationBudget,
) -> Result<Vec<Vec<Annotation>>, ParseError> {
    let count = reader.u1()?;
    let mut result = Vec::new();
    // This table need not match the descriptor (JVMS 4.7.18/19).
    for _ in 0..count {
        result.push(annotations(reader, pool, budget)?);
    }
    Ok(result)
}
fn offset(reader: &mut Reader<'_>, code: Option<CodeContext>) -> Result<u16, ParseError> {
    let offset = reader.u2()?;
    if !code.is_some_and(|code| usize::from(offset) < code.length) {
        return Err(reader.invalid("type-annotation offset is outside Code"));
    }
    Ok(offset)
}
fn target(
    reader: &mut Reader<'_>,
    location: Location,
    code: Option<CodeContext>,
) -> Result<TypeAnnotationTarget, ParseError> {
    let tag = reader.u1()?;
    let valid = match location {
        Location::Class => matches!(tag, 0x00 | 0x10 | 0x11),
        Location::Field | Location::RecordComponent => tag == 0x13,
        Location::Method => matches!(tag, 0x01 | 0x12 | 0x14..=0x17),
        Location::Code => matches!(tag, 0x40..=0x4b),
    };
    if !valid {
        return Err(reader.invalid("invalid type-annotation target_type for this location"));
    }
    Ok(match tag {
        0x00 => TypeAnnotationTarget::ClassTypeParameter(reader.u1()?),
        0x01 => TypeAnnotationTarget::MethodTypeParameter(reader.u1()?),
        0x10 => TypeAnnotationTarget::Supertype(reader.u2()?),
        0x11 => TypeAnnotationTarget::ClassTypeParameterBound {
            type_parameter_index: reader.u1()?,
            bound_index: reader.u1()?,
        },
        0x12 => TypeAnnotationTarget::MethodTypeParameterBound {
            type_parameter_index: reader.u1()?,
            bound_index: reader.u1()?,
        },
        0x13 => TypeAnnotationTarget::Field,
        0x14 => TypeAnnotationTarget::MethodReturn,
        0x15 => TypeAnnotationTarget::MethodReceiver,
        0x16 => TypeAnnotationTarget::MethodFormalParameter(reader.u1()?),
        0x17 => TypeAnnotationTarget::Throws(reader.u2()?),
        0x40 | 0x41 => {
            let code =
                code.ok_or_else(|| reader.invalid("local-variable annotation requires Code"))?;
            let count = reader.u2()?;
            let mut table = Vec::new();
            for _ in 0..count {
                let start_pc = reader.u2()?;
                let length = reader.u2()?;
                let index = reader.u2()?;
                if usize::from(start_pc) >= code.length
                    || usize::from(start_pc) + usize::from(length) > code.length
                {
                    return Err(
                        reader.invalid("type-annotation local-variable range is outside Code")
                    );
                }
                if index >= code.max_locals {
                    return Err(reader
                        .invalid("type-annotation local-variable index is outside max_locals"));
                }
                table.push(LocalVariableTarget {
                    start_pc,
                    length,
                    index,
                });
            }
            if tag == 0x40 {
                TypeAnnotationTarget::LocalVariable(table)
            } else {
                TypeAnnotationTarget::ResourceVariable(table)
            }
        }
        0x42 => {
            let index = reader.u2()?;
            if !code.is_some_and(|code| usize::from(index) < code.exception_count) {
                return Err(
                    reader.invalid("type-annotation catch index is outside the exception table")
                );
            }
            TypeAnnotationTarget::ExceptionParameter(index)
        }
        0x43 => TypeAnnotationTarget::InstanceOf(offset(reader, code)?),
        0x44 => TypeAnnotationTarget::New(offset(reader, code)?),
        0x45 => TypeAnnotationTarget::ConstructorReference(offset(reader, code)?),
        0x46 => TypeAnnotationTarget::MethodReference(offset(reader, code)?),
        0x47 => TypeAnnotationTarget::Cast {
            offset: offset(reader, code)?,
            type_argument_index: reader.u1()?,
        },
        0x48 => TypeAnnotationTarget::ConstructorInvocationTypeArgument {
            offset: offset(reader, code)?,
            type_argument_index: reader.u1()?,
        },
        0x49 => TypeAnnotationTarget::MethodInvocationTypeArgument {
            offset: offset(reader, code)?,
            type_argument_index: reader.u1()?,
        },
        0x4a => TypeAnnotationTarget::ConstructorReferenceTypeArgument {
            offset: offset(reader, code)?,
            type_argument_index: reader.u1()?,
        },
        0x4b => TypeAnnotationTarget::MethodReferenceTypeArgument {
            offset: offset(reader, code)?,
            type_argument_index: reader.u1()?,
        },
        _ => return Err(reader.invalid("invalid type-annotation target_type")),
    })
}
pub(crate) fn type_annotations(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    location: Location,
    code: Option<CodeContext>,
    budget: &mut AnnotationBudget,
) -> Result<Vec<TypeAnnotation>, ParseError> {
    let count = reader.u2()?;
    let mut result = Vec::new();
    for _ in 0..count {
        let target = target(reader, location, code)?;
        let length = reader.u1()?;
        let mut path = Vec::new();
        for _ in 0..length {
            let kind = reader.u1()?;
            let type_argument_index = reader.u1()?;
            let kind = match kind {
                0 => TypePathKind::Array,
                1 => TypePathKind::InnerType,
                2 => TypePathKind::WildcardBound,
                3 => TypePathKind::TypeArgument,
                _ => return Err(reader.invalid("invalid type_path_kind")),
            };
            if kind != TypePathKind::TypeArgument && type_argument_index != 0 {
                return Err(reader
                    .invalid("non-type-argument path entry must have zero type_argument_index"));
            }
            path.push(TypePathEntry {
                kind,
                type_argument_index,
            });
        }
        let annotation = annotation(reader, pool, budget, 1)?;
        result.push(TypeAnnotation {
            target,
            path,
            annotation,
        });
    }
    Ok(result)
}
