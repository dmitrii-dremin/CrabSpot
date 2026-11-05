//! Compact computational shapes and temporary borrowed signature identities.
use crate::{
    ClassId, FieldAccessError, FieldResolutionContext, HeapError, MethodAccessError as E, MethodId,
    Reference, ReferenceType, RegistryError, TypeAccessError, Value, ValueKind,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceInvocationKind {
    Virtual,
    Interface,
    Special,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodReturnKind {
    Void,
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Reference,
}
impl MethodReturnKind {
    fn value_kind(self) -> Option<ValueKind> {
        match self {
            Self::Void => None,
            Self::Long => Some(ValueKind::Long),
            Self::Double => Some(ValueKind::Double),
            Self::Float => Some(ValueKind::Float),
            Self::Reference => Some(ValueKind::Reference),
            _ => Some(ValueKind::Int),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodValueShape {
    pub parameters: Vec<ValueKind>,
    pub parameter_slots: u16,
    pub result: MethodReturnKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodValuePosition {
    Argument(usize),
    Return,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodReferenceMismatch {
    pub method: MethodId,
    pub position: MethodValuePosition,
    pub source: ReferenceType,
    pub target: ReferenceType,
}

fn malformed() -> E {
    FieldAccessError::InvalidState("invalid borrowed method descriptor").into()
}
fn allocation() -> E {
    TypeAccessError::Heap(HeapError::AllocationFailure {
        resource: "method signature metadata",
    })
    .into()
}
fn reserve<T>(values: &mut Vec<T>, amount: usize) -> Result<(), E> {
    values.try_reserve(amount).map_err(|_| allocation())
}
fn reserve_seen(values: &mut HashSet<ClassId>, amount: usize) -> Result<(), E> {
    values.try_reserve(amount).map_err(|_| allocation())
}
#[derive(Clone, Copy)]
struct Span<'a> {
    units: &'a [u16],
    kind: ValueKind,
}
pub(crate) struct Signature<'a> {
    parameters: [Option<Span<'a>>; 255],
    len: usize,
    slots: u16,
    pub(crate) result: MethodReturnKind,
    result_span: Option<Span<'a>>,
    pub(crate) nominal: bool,
}
impl<'a> Signature<'a> {
    pub(crate) fn shape(&self) -> Result<MethodValueShape, E> {
        let mut parameters = Vec::new();
        reserve(&mut parameters, self.len)?;
        for index in 0..self.len {
            parameters.push(self.parameter(index)?.kind);
        }
        Ok(MethodValueShape {
            parameters,
            parameter_slots: self.slots,
            result: self.result,
        })
    }
    fn parameter(&self, index: usize) -> Result<Span<'a>, E> {
        self.parameters
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(malformed)
    }
}
// Numeric descriptors reach their numeric end (or the first reference marker)
// within 258 units: '(' + at most255 parameter units + ')' + one return tag.
fn reference_prefix(units: &[u16]) -> Result<bool, E> {
    if units.first() != Some(&40) {
        return Err(malformed());
    }
    let mut index = 1usize;
    let mut slots = 0u16;
    while index < 258 {
        let tag = *units.get(index).ok_or_else(malformed)?;
        match tag {
            76 | 91 => return Ok(true),
            41 => {
                let result = *units.get(index + 1).ok_or_else(malformed)?;
                if matches!(result, 76 | 91) {
                    return Ok(true);
                }
                if numeric_tag(result).is_none() && result != 86 {
                    return Err(malformed());
                }
                return if index + 2 == units.len() {
                    Ok(false)
                } else {
                    Err(malformed())
                };
            }
            _ => {
                let (kind, _) = numeric_tag(tag).ok_or_else(malformed)?;
                slots = slots
                    .checked_add(kind.slots() as u16)
                    .filter(|slots| *slots <= 255)
                    .ok_or_else(malformed)?;
                index += 1;
            }
        }
    }
    Err(malformed())
}
fn numeric_tag(tag: u16) -> Option<(ValueKind, MethodReturnKind)> {
    Some(match tag {
        90 => (ValueKind::Int, MethodReturnKind::Boolean),
        66 => (ValueKind::Int, MethodReturnKind::Byte),
        67 => (ValueKind::Int, MethodReturnKind::Char),
        83 => (ValueKind::Int, MethodReturnKind::Short),
        73 => (ValueKind::Int, MethodReturnKind::Int),
        74 => (ValueKind::Long, MethodReturnKind::Long),
        70 => (ValueKind::Float, MethodReturnKind::Float),
        68 => (ValueKind::Double, MethodReturnKind::Double),
        _ => return None,
    })
}
fn field_span<'a>(
    units: &'a [u16],
    index: &mut usize,
) -> Result<(Span<'a>, MethodReturnKind, bool), E> {
    let start = *index;
    let mut rank = 0usize;
    while units.get(*index) == Some(&91) {
        rank += 1;
        if rank > 255 {
            return Err(malformed());
        }
        *index = index.checked_add(1).ok_or_else(malformed)?;
    }
    let tag = *units.get(*index).ok_or_else(malformed)?;
    *index = index.checked_add(1).ok_or_else(malformed)?;
    let nominal = tag == 76;
    let (mut kind, mut result) = if nominal {
        let begin = *index;
        while units.get(*index).is_some_and(|unit| *unit != 59) {
            *index = index.checked_add(1).ok_or_else(malformed)?;
        }
        let name = units.get(begin..*index).ok_or_else(malformed)?;
        if name.is_empty()
            || !name.split(|unit| *unit == 47).all(|part| {
                !part.is_empty() && !part.iter().any(|unit| matches!(*unit, 46 | 59 | 91))
            })
            || units.get(*index) != Some(&59)
        {
            return Err(malformed());
        }
        *index = index.checked_add(1).ok_or_else(malformed)?;
        (ValueKind::Reference, MethodReturnKind::Reference)
    } else {
        numeric_tag(tag).ok_or_else(malformed)?
    };
    if rank > 0 {
        kind = ValueKind::Reference;
        result = MethodReturnKind::Reference;
    }
    Ok((
        Span {
            units: units.get(start..*index).ok_or_else(malformed)?,
            kind,
        },
        result,
        nominal,
    ))
}
fn parse_signature(units: &[u16]) -> Result<Signature<'_>, E> {
    if units.first() != Some(&40) {
        return Err(malformed());
    }
    let mut signature = Signature {
        parameters: [None; 255],
        len: 0,
        slots: 0,
        result: MethodReturnKind::Void,
        result_span: None,
        nominal: false,
    };
    let mut index = 1usize;
    while units.get(index) != Some(&41) {
        let (span, _, nominal) = field_span(units, &mut index)?;
        signature.slots = signature
            .slots
            .checked_add(span.kind.slots() as u16)
            .filter(|slots| *slots <= 255)
            .ok_or_else(malformed)?;
        *signature
            .parameters
            .get_mut(signature.len)
            .ok_or_else(malformed)? = Some(span);
        signature.len += 1;
        signature.nominal |= nominal;
    }
    index = index.checked_add(1).ok_or_else(malformed)?;
    if units.get(index) == Some(&86) {
        index = index.checked_add(1).ok_or_else(malformed)?;
    } else {
        let (span, result, nominal) = field_span(units, &mut index)?;
        signature.result = result;
        signature.result_span = Some(span);
        signature.nominal |= nominal;
    }
    if index != units.len() {
        return Err(malformed());
    }
    Ok(signature)
}

impl<'a> FieldResolutionContext<'a> {
    pub(crate) fn inspect_signature(&mut self, units: &'a [u16]) -> Result<Signature<'a>, E> {
        if reference_prefix(units)? {
            for _ in units {
                self.step()?;
            }
        }
        parse_signature(units)
    }
    fn declared_signature(&mut self, method: MethodId) -> Result<Signature<'a>, E> {
        self.unchanged()?;
        let registry = self.registry();
        let definition = registry.class(method.owner())?;
        let member = registry.method(method)?;
        self.heap().instance_graph_node(method.owner())?;
        let descriptor = definition
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?;
        self.inspect_signature(descriptor.units())
    }
    pub fn method_value_shape(&mut self, method: MethodId) -> Result<MethodValueShape, E> {
        self.declared_signature(method)?.shape()
    }
    pub fn check_method_reference_profile(
        &mut self,
        method: MethodId,
        receiver: Option<Reference>,
    ) -> Result<(), E> {
        let signature = self.declared_signature(method)?;
        let member = self.registry().method(method)?;
        if (member.access_flags & 8 == 0) != receiver.is_some() {
            return Err(FieldAccessError::InvalidState(
                "method profile receiver Option does not match staticness",
            )
            .into());
        }
        if let Some(receiver) = receiver {
            self.heap()
                .check_class_mirror_method_entry(method, receiver)?;
        }
        if !signature.nominal || member.access_flags & 8 != 0 {
            return Ok(());
        }
        let receiver = receiver.ok_or_else(|| {
            E::from(FieldAccessError::InvalidState(
                "instance method profile has no receiver",
            ))
        })?;
        if self.class_sites_enabled() && self.get_class_signature(method)? {
            if let Some(actual @ crate::ReferenceType::Array { .. }) =
                self.reference_type(receiver)?
            {
                if self.is_assignable(actual, crate::ReferenceType::Class(method.owner()))? {
                    return Ok(());
                }
                return Err(FieldAccessError::InvalidState(
                    "array does not implement native Object slot",
                )
                .into());
            }
        }
        let actual = self.heap().instance_class(receiver)?;
        self.require_method_receiver(actual, method.owner())?;
        if self.is_member_checked() {
            return Ok(());
        }
        self.audit_reference_method(method, actual)
    }
    fn audit_reference_method(&mut self, method: MethodId, actual: ClassId) -> Result<(), E> {
        let registry = self.registry();
        let target = registry.method(method)?;
        let owner = registry.class(method.owner())?;
        let name = owner
            .constant_pool
            .utf8(target.name_index)
            .map_err(RegistryError::Parse)?;
        let descriptor = owner
            .constant_pool
            .utf8(target.descriptor_index)
            .map_err(RegistryError::Parse)?;
        let loader = registry.defining_loader(method.owner())?;
        let mut work = Vec::new();
        reserve(&mut work, 1)?;
        work.push(actual);
        let mut visited = HashSet::new();
        while let Some(class) = work.pop() {
            if visited.contains(&class) {
                continue;
            }
            self.step()?;
            reserve_seen(&mut visited, 1)?;
            visited.insert(class);
            let definition = registry.class(class)?;
            for (index, candidate) in definition.methods.iter().enumerate() {
                self.step()?;
                let candidate_name = definition
                    .constant_pool
                    .utf8(candidate.name_index)
                    .map_err(RegistryError::Parse)?;
                for _ in candidate_name.units() {
                    self.step()?;
                }
                if candidate_name != name {
                    continue;
                }
                let candidate_descriptor = definition
                    .constant_pool
                    .utf8(candidate.descriptor_index)
                    .map_err(RegistryError::Parse)?;
                for _ in candidate_descriptor.units() {
                    self.step()?;
                }
                if candidate_descriptor == descriptor && registry.defining_loader(class)? != loader
                {
                    return Err(E::UnsupportedOverrideConstraint {
                        method,
                        declaration: MethodId {
                            owner: class,
                            index,
                        },
                    });
                }
            }
            let len = self.heap().instance_graph_node(class)?.interfaces.len();
            for index in (0..len).rev() {
                self.step()?;
                let interface = *self
                    .heap()
                    .instance_graph_node(class)?
                    .interfaces
                    .get(index)
                    .ok_or_else(|| {
                        E::from(FieldAccessError::InvalidState(
                            "profile interface edge disappeared",
                        ))
                    })?;
                reserve(&mut work, 1)?;
                work.push(interface);
            }
            if let Some(parent) = self.heap().instance_graph_node(class)?.superclass {
                self.step()?;
                reserve(&mut work, 1)?;
                work.push(parent);
            }
        }
        Ok(())
    }
    pub fn check_method_arguments(
        &mut self,
        method: MethodId,
        arguments: &[Value],
    ) -> Result<(), E> {
        let signature = self.declared_signature(method)?;
        if arguments.len() != signature.len {
            return Err(E::ArgumentCount {
                method,
                expected: signature.len,
                actual: arguments.len(),
            });
        }
        for (index, value) in arguments.iter().enumerate() {
            let expected = signature.parameter(index)?.kind;
            if value.kind() != expected {
                return Err(E::ValueKindMismatch {
                    method,
                    position: MethodValuePosition::Argument(index),
                    expected: Some(expected),
                    actual: Some(value.kind()),
                });
            }
        }
        for (index, value) in arguments.iter().enumerate() {
            if let Value::Reference(reference) = value {
                self.check_signature_reference(
                    method,
                    MethodValuePosition::Argument(index),
                    *reference,
                    signature.parameter(index)?.units,
                )?;
            }
        }
        Ok(())
    }
    pub fn check_method_return(&mut self, method: MethodId, value: Option<Value>) -> Result<(), E> {
        let signature = self.declared_signature(method)?;
        let expected = signature.result.value_kind();
        let actual = value.map(Value::kind);
        if expected != actual {
            return Err(E::ValueKindMismatch {
                method,
                position: MethodValuePosition::Return,
                expected,
                actual,
            });
        }
        if let Some(Value::Reference(reference)) = value {
            let span = signature.result_span.ok_or_else(malformed)?;
            self.check_signature_reference(
                method,
                MethodValuePosition::Return,
                reference,
                span.units,
            )?;
        }
        Ok(())
    }
    fn check_signature_reference(
        &mut self,
        method: MethodId,
        position: MethodValuePosition,
        reference: Reference,
        span: &'a [u16],
    ) -> Result<(), E> {
        if reference.is_null() {
            return Ok(());
        }
        let source = self.reference_type(reference)?.ok_or_else(|| {
            E::from(TypeAccessError::InvalidType(
                "nonnull method value has no runtime type",
            ))
        })?;
        let target = self.resolve_reference_span(method.owner(), span)?;
        if self.is_assignable(source, target)? {
            Ok(())
        } else {
            Err(E::IncompatibleReference(Box::new(
                MethodReferenceMismatch {
                    method,
                    position,
                    source,
                    target,
                },
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DefinitionRegistry, FieldResolutionOptions, FieldResource, HeapOptions, HierarchyOptions,
        InstanceOptions, ManagedHeap, PrimitiveArrayKind, ReferenceTypeUniverse, RegistryOptions,
        SameLoaderResolver, build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn fixture() -> (DefinitionRegistry, ManagedHeap, MethodId) {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
        let mut classes = Vec::new();
        for (name, flags, parent, methods) in [
            ("java/lang/Object", 0x21_u16, 0_u16, 1_u16),
            ("java/lang/Cloneable", 0x601, 4, 0),
            ("java/io/Serializable", 0x601, 4, 0),
        ] {
            let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 7];
            for (index, text) in [(1, name), (3, "java/lang/Object")] {
                bytes.push(1);
                bytes.extend((text.len() as u16).to_be_bytes());
                bytes.extend(text.bytes());
                bytes.extend([7, 0, index]);
            }
            for text in ["m", descriptor] {
                bytes.push(1);
                bytes.extend((text.len() as u16).to_be_bytes());
                bytes.extend(text.bytes());
            }
            for word in [flags, 2, parent, 0, 0, methods] {
                bytes.extend(word.to_be_bytes());
            }
            if methods == 1 {
                for word in [0x101_u16, 5, 6, 0] {
                    bytes.extend(word.to_be_bytes());
                }
            }
            bytes.extend([0, 0]);
            classes.push(
                registry
                    .define(loader, &units(name), &bytes, ParseOptions::default())
                    .unwrap(),
            );
        }
        let plans: Vec<_> = classes
            .iter()
            .map(|class| {
                build_hierarchy(
                    &registry,
                    *class,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(
            &registry,
            ReferenceTypeUniverse {
                bootstrap_loader: loader,
                object: classes[0],
                cloneable: classes[1],
                serializable: classes[2],
            },
        )
        .unwrap();
        (
            registry,
            heap,
            MethodId {
                owner: classes[0],
                index: 0,
            },
        )
    }

    #[test]
    fn bounded_prefix_and_borrowed_spans_cover_slot_rank_and_grammar_boundaries() {
        let maximum = units(&format!("({})V", "I".repeat(255)));
        assert_eq!(maximum.len(), 258);
        assert_eq!(reference_prefix(&maximum), Ok(false));
        let parsed = parse_signature(&maximum).unwrap();
        assert_eq!((parsed.len, parsed.slots), (255, 255));
        assert_eq!(
            parsed.shape().unwrap().parameters,
            vec![ValueKind::Int; 255]
        );
        assert!(parsed.parameter(255).is_err());
        let borrowed = units("(J[[Lp/Target;D)[I");
        let parsed = parse_signature(&borrowed).unwrap();
        assert_eq!((parsed.len, parsed.slots, parsed.nominal), (3, 5, true));
        assert_eq!(
            parsed.parameter(1).unwrap().units.as_ptr(),
            borrowed[2..].as_ptr()
        );
        assert_eq!(parsed.parameter(1).unwrap().units, units("[[Lp/Target;"));
        assert_eq!(parsed.result_span.unwrap().units, units("[I"));
        let maximum_rank = units(&format!("({}I)V", "[".repeat(255)));
        assert_eq!(parse_signature(&maximum_rank).unwrap().slots, 1);
        for invalid in [
            "",
            "I",
            "(V)V",
            "()",
            "()VV",
            "(L;)V",
            "(La//b;)V",
            "(La.b;)V",
            "(La[b;)V",
            "(La)V",
            "([V)V",
            "(I)Vx",
        ] {
            assert!(parse_signature(&units(invalid)).is_err(), "{invalid}");
        }
        for invalid in [
            format!("({})V", "I".repeat(256)),
            format!("({})V", "J".repeat(128)),
            format!("({}I)V", "[".repeat(256)),
        ] {
            assert!(parse_signature(&units(&invalid)).is_err());
        }
        let mut early_reference = units("(L");
        early_reference.resize(65_535, 0);
        assert_eq!(reference_prefix(&early_reference), Ok(true));
        let invalid_numeric = units(&format!("({}X", "I".repeat(256)));
        assert!(reference_prefix(&invalid_numeric).is_err());
    }

    #[test]
    fn reference_budget_is_spent_before_full_parsing_and_numeric_checks_spend_none() {
        let (registry, mut heap, _) = fixture();
        let descriptor = units("(Lbroken");
        let numeric_invalid = units("(V)V");
        let mut resolver = SameLoaderResolver;
        let seed = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap()
        .lookup_steps();
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: seed + descriptor.len() - 1,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        assert!(ctx.inspect_signature(&numeric_invalid).is_err());
        assert_eq!(ctx.lookup_steps(), seed);
        assert!(
            matches!(ctx.inspect_signature(&descriptor), Err(E::Context(FieldAccessError::ResourceLimit {resource:FieldResource::LookupSteps,limit,required})) if limit==seed+descriptor.len()-1 && required==seed+descriptor.len())
        );
        assert_eq!(ctx.lookup_steps(), seed + descriptor.len() - 1);
        drop(ctx);
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: seed + descriptor.len(),
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        assert!(matches!(
            ctx.inspect_signature(&descriptor),
            Err(E::Context(FieldAccessError::InvalidState(_)))
        ));
        assert_eq!(ctx.lookup_steps(), seed + descriptor.len());
    }

    #[test]
    fn temporary_capacity_overflow_keeps_existing_compact_and_audit_entries() {
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let mut compact = vec![ValueKind::Reference];
        let mut work = vec![class];
        let mut seen = HashSet::from([class]);
        assert_eq!(reserve(&mut compact, usize::MAX), Err(allocation()));
        assert_eq!(reserve(&mut work, usize::MAX), Err(allocation()));
        assert_eq!(reserve_seen(&mut seen, usize::MAX), Err(allocation()));
        assert_eq!(compact, [ValueKind::Reference]);
        assert_eq!(work, [class]);
        assert_eq!(seen, HashSet::from([class]));
    }

    #[test]
    fn method_and_source_components_are_checked_and_new_errors_keep_their_sources() {
        let (registry, mut heap, method) = fixture();
        let valid = heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let (identity, index, generation) = valid.heap_ref().unwrap().components();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        for (reference, error) in [
            (
                Reference::issued(identity, usize::MAX, generation),
                HeapError::MissingEntry { index: usize::MAX },
            ),
            (
                Reference::issued(identity, index, generation + 1),
                HeapError::StaleGeneration {
                    expected: generation,
                    actual: generation + 1,
                },
            ),
        ] {
            let expected = E::Type(Box::new(TypeAccessError::Heap(error)));
            assert_eq!(
                ctx.check_method_arguments(method, &[Value::Reference(reference)]),
                Err(expected.clone())
            );
            assert_eq!(
                ctx.check_method_return(method, Some(Value::Reference(reference))),
                Err(expected.clone())
            );
            assert!(expected.source().is_some());
            assert!(expected.to_string().contains("reference type validation"));
        }
        for forged in [
            MethodId {
                index: usize::MAX,
                ..method
            },
            MethodId {
                owner: ClassId {
                    index: usize::MAX,
                    ..method.owner()
                },
                index: 0,
            },
            MethodId {
                owner: ClassId {
                    registry: method.owner().registry + 1,
                    ..method.owner()
                },
                index: 0,
            },
        ] {
            assert!(matches!(
                ctx.method_value_shape(forged),
                Err(E::Context(FieldAccessError::Registry(
                    RegistryError::InvalidHandle { .. }
                )))
            ));
            assert!(matches!(
                ctx.check_method_arguments(forged, &[]),
                Err(E::Context(FieldAccessError::Registry(
                    RegistryError::InvalidHandle { .. }
                )))
            ));
            assert!(matches!(
                ctx.check_method_return(forged, None),
                Err(E::Context(FieldAccessError::Registry(
                    RegistryError::InvalidHandle { .. }
                )))
            ));
        }
        let nested = E::from(TypeAccessError::Java(crate::TypeJavaError::ClassCast));
        assert!(matches!(nested, E::Type(_)));
        assert!(nested.source().is_some());
        let mismatch = E::IncompatibleReference(Box::new(MethodReferenceMismatch {
            method,
            position: MethodValuePosition::Return,
            source: ReferenceType::Class(method.owner()),
            target: ReferenceType::Class(method.owner()),
        }));
        assert!(mismatch.source().is_none());
        assert!(mismatch.to_string().contains("Return"));
    }
}
