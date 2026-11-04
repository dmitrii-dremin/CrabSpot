//! Checked reference identities and bounded assignment, separate from protected
//! class-only access checks and from allocation/initialization protocols.
use crate::{
    ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext, HeapError,
    InstanceError, LoaderId, ManagedHeap, PrimitiveArrayKind, Reference, RegistryError,
};
use crabspot_classfile::Constant;
use std::{collections::HashSet, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceTypeUniverse {
    pub bootstrap_loader: LoaderId,
    pub object: ClassId,
    pub cloneable: ClassId,
    pub serializable: ClassId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ArrayElement {
    Primitive(PrimitiveArrayKind),
    Class(ClassId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReferenceType {
    Class(ClassId),
    Array {
        dimensions: u8,
        element: ArrayElement,
        defining_loader: LoaderId,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeJavaError {
    IllegalAccess,
    ClassCast,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeAccessError {
    Java(TypeJavaError),
    Context(FieldAccessError),
    Heap(HeapError),
    MissingUniverse,
    UniverseConflict,
    InvalidUniverse(&'static str),
    InvalidType(&'static str),
    InvalidConstant { origin: ClassId, index: u16 },
}
impl fmt::Display for TypeAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Java(error) => write!(f, "reference type operation failed: {error:?}"),
            Self::Context(error) => write!(f, "reference type context failed: {error}"),
            Self::Heap(error) => write!(f, "reference type heap operation failed: {error}"),
            Self::MissingUniverse => {
                f.write_str("bind a reference type universe before a non-null type operation")
            }
            Self::UniverseConflict => {
                f.write_str("heap is already bound to a different reference type universe")
            }
            Self::InvalidUniverse(reason) => write!(f, "invalid reference type universe: {reason}"),
            Self::InvalidType(reason) => write!(f, "invalid reference type: {reason}"),
            Self::InvalidConstant { origin, index } => write!(
                f,
                "constant #{index} in {origin:?} is not a valid Class reference"
            ),
        }
    }
}
impl std::error::Error for TypeAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            Self::Heap(error) => Some(error),
            _ => None,
        }
    }
}
impl From<FieldAccessError> for TypeAccessError {
    fn from(error: FieldAccessError) -> Self {
        Self::Context(error)
    }
}
impl From<RegistryError> for TypeAccessError {
    fn from(error: RegistryError) -> Self {
        Self::Context(FieldAccessError::Registry(error))
    }
}
impl From<InstanceError> for TypeAccessError {
    fn from(error: InstanceError) -> Self {
        Self::Context(FieldAccessError::Instance(error))
    }
}
impl From<HeapError> for TypeAccessError {
    fn from(error: HeapError) -> Self {
        Self::Heap(error)
    }
}
fn allocation() -> TypeAccessError {
    HeapError::AllocationFailure {
        resource: "reference type traversal",
    }
    .into()
}
fn push(values: &mut Vec<ClassId>, class: ClassId) -> Result<(), TypeAccessError> {
    reserve_work(values, 1)?;
    values.push(class);
    Ok(())
}
fn reserve_work(values: &mut Vec<ClassId>, amount: usize) -> Result<(), TypeAccessError> {
    values.try_reserve(amount).map_err(|_| allocation())
}
fn reserve_seen(values: &mut HashSet<ClassId>, amount: usize) -> Result<(), TypeAccessError> {
    values.try_reserve(amount).map_err(|_| allocation())
}
impl ManagedHeap {
    pub fn reference_type_universe(&self) -> Option<ReferenceTypeUniverse> {
        self.reference_universe
    }
    pub fn bind_reference_types(
        &mut self,
        registry: &DefinitionRegistry,
        universe: ReferenceTypeUniverse,
    ) -> Result<(), TypeAccessError> {
        if self
            .reference_universe
            .is_some_and(|previous| previous != universe)
        {
            return Err(TypeAccessError::UniverseConflict);
        }
        // A valid name probes the loader handle without changing any definition.
        registry.lookup_class(universe.bootstrap_loader, &[88])?;
        if self.instance_graph_registry()? != Some(registry.identity()) {
            return Err(TypeAccessError::InvalidUniverse(
                "admitted layouts must belong to the selected registry",
            ));
        }
        for (class, name, interface) in [
            (universe.object, "java/lang/Object", false),
            (universe.cloneable, "java/lang/Cloneable", true),
            (universe.serializable, "java/io/Serializable", true),
        ] {
            let definition = registry.class(class)?;
            let node = self.instance_graph_node(class)?;
            if registry.defining_loader(class)? != universe.bootstrap_loader
                || !definition
                    .name()
                    .map_err(RegistryError::Parse)?
                    .equals_ascii(name)
                || definition.access_flags & 1 == 0
                || (definition.access_flags & 0x200 != 0) != interface
                || node.flags != definition.access_flags
            {
                return Err(TypeAccessError::InvalidUniverse(
                    "core name, public kind or defining loader differs from its designated role",
                ));
            }
            if interface {
                if definition.access_flags & 0x400 == 0
                    || node.superclass != Some(universe.object)
                    || !node.interfaces.is_empty()
                {
                    return Err(TypeAccessError::InvalidUniverse(
                        "array marker must be abstract with only the selected Object dependency",
                    ));
                }
            } else if node.superclass.is_some() {
                return Err(TypeAccessError::InvalidUniverse(
                    "Object must have no superclass",
                ));
            }
        }
        for class in self.instance_graph_classes()? {
            if self.instance_graph_node(class)?.superclass.is_none() && class != universe.object {
                return Err(TypeAccessError::InvalidUniverse(
                    "admitted graph contains a different Object root",
                ));
            }
        }
        self.reference_universe = Some(universe);
        Ok(())
    }
}

impl<'a> FieldResolutionContext<'a> {
    fn universe(&self) -> Result<ReferenceTypeUniverse, TypeAccessError> {
        self.heap()
            .reference_type_universe()
            .ok_or(TypeAccessError::MissingUniverse)
    }
    fn nominal(&mut self, class: ClassId) -> Result<(), TypeAccessError> {
        self.step()?;
        self.registry().class(class)?;
        self.heap().instance_graph_node(class)?;
        Ok(())
    }
    fn validate_reference_type(
        &mut self,
        value: ReferenceType,
        universe: ReferenceTypeUniverse,
    ) -> Result<(), TypeAccessError> {
        match value {
            ReferenceType::Class(class) => self.nominal(class),
            ReferenceType::Array {
                dimensions,
                element,
                defining_loader,
            } => {
                self.step()?;
                if dimensions == 0 {
                    return Err(TypeAccessError::InvalidType(
                        "array rank must be 1 through 255",
                    ));
                }
                self.registry().lookup_class(defining_loader, &[88])?;
                let expected = match element {
                    ArrayElement::Primitive(_) => universe.bootstrap_loader,
                    ArrayElement::Class(class) => {
                        self.nominal(class)?;
                        self.registry().defining_loader(class)?
                    }
                };
                if defining_loader != expected {
                    return Err(TypeAccessError::InvalidType(
                        "array defining loader differs from its leaf identity",
                    ));
                }
                Ok(())
            }
        }
    }
    fn type_name(&self, origin: ClassId, index: u16) -> Result<&'a [u16], TypeAccessError> {
        self.unchanged()?;
        let definition = self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        let invalid = || TypeAccessError::InvalidConstant { origin, index };
        let Constant::Class { name_index } =
            definition.constant_pool.get(index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        Ok(definition
            .constant_pool
            .utf8(*name_index)
            .map_err(|_| invalid())?
            .units())
    }
    fn accessible_nominal(
        &mut self,
        origin: ClassId,
        name: &'a [u16],
    ) -> Result<ClassId, TypeAccessError> {
        let class = self.resolve_type_name(origin, name)?;
        self.nominal(class)?;
        if self.registry().class(class)?.access_flags & 1 == 0
            && !self.same_package(origin, class)?
        {
            return Err(TypeAccessError::Java(TypeJavaError::IllegalAccess));
        }
        Ok(class)
    }
    fn resolve_type_name_value(
        &mut self,
        origin: ClassId,
        name: &'a [u16],
    ) -> Result<ReferenceType, TypeAccessError> {
        let universe = self.universe()?;
        for _ in name {
            self.step()?;
        }
        if name.first() != Some(&91) {
            return Ok(ReferenceType::Class(self.accessible_nominal(origin, name)?));
        }
        let rank = name.iter().take_while(|unit| **unit == 91).count();
        let dimensions = u8::try_from(rank)
            .map_err(|_| TypeAccessError::InvalidType("array rank exceeds 255"))?;
        let leaf = name
            .get(rank..)
            .ok_or(TypeAccessError::InvalidType("array leaf is absent"))?;
        let primitive = match leaf {
            [90] => Some(PrimitiveArrayKind::Boolean),
            [66] => Some(PrimitiveArrayKind::Byte),
            [67] => Some(PrimitiveArrayKind::Char),
            [83] => Some(PrimitiveArrayKind::Short),
            [73] => Some(PrimitiveArrayKind::Int),
            [74] => Some(PrimitiveArrayKind::Long),
            [70] => Some(PrimitiveArrayKind::Float),
            [68] => Some(PrimitiveArrayKind::Double),
            _ => None,
        };
        let (element, defining_loader) = if let Some(kind) = primitive {
            (ArrayElement::Primitive(kind), universe.bootstrap_loader)
        } else {
            let Some(class_name) = leaf
                .strip_prefix(&[76])
                .and_then(|value| value.strip_suffix(&[59]))
            else {
                return Err(TypeAccessError::InvalidType(
                    "array leaf is not a primitive or nominal descriptor",
                ));
            };
            if class_name.is_empty()
                || !class_name.split(|unit| *unit == 47).all(|part| {
                    !part.is_empty() && !part.iter().any(|unit| matches!(*unit, 46 | 59 | 91))
                })
            {
                return Err(TypeAccessError::InvalidType(
                    "invalid nominal array leaf name",
                ));
            }
            let class = self.accessible_nominal(origin, class_name)?;
            (
                ArrayElement::Class(class),
                self.registry().defining_loader(class)?,
            )
        };
        Ok(ReferenceType::Array {
            dimensions,
            element,
            defining_loader,
        })
    }
    pub fn resolve_reference_type(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<ReferenceType, TypeAccessError> {
        let name = self.type_name(origin, index)?;
        self.resolve_type_name_value(origin, name)
    }
    pub fn reference_type(
        &mut self,
        reference: Reference,
    ) -> Result<Option<ReferenceType>, TypeAccessError> {
        self.unchanged()?;
        if reference.is_null() {
            return Ok(None);
        }
        let universe = self.universe()?;
        let value = match &self.heap().entry(reference)?.payload {
            crate::heap::EntryPayload::Instance(_) => {
                let class = self.heap().instance_class(reference)?;
                self.nominal(class)?;
                ReferenceType::Class(class)
            }
            crate::heap::EntryPayload::Array(_) => ReferenceType::Array {
                dimensions: 1,
                element: ArrayElement::Primitive(self.heap().kind(reference)?),
                defining_loader: universe.bootstrap_loader,
            },
        };
        Ok(Some(value))
    }
    fn nominal_assignment(
        &mut self,
        source: ClassId,
        target: ClassId,
    ) -> Result<bool, TypeAccessError> {
        let mut work = Vec::new();
        push(&mut work, source)?;
        let mut visited = HashSet::new();
        while let Some(class) = work.pop() {
            if visited.contains(&class) {
                continue;
            }
            self.step()?;
            if class == target {
                return Ok(true);
            }
            reserve_seen(&mut visited, 1)?;
            visited.insert(class);
            let len = self.heap().instance_graph_node(class)?.interfaces.len();
            for index in (0..len).rev() {
                self.step()?;
                let interface = *self
                    .heap()
                    .instance_graph_node(class)?
                    .interfaces
                    .get(index)
                    .ok_or(TypeAccessError::InvalidType("interface edge disappeared"))?;
                push(&mut work, interface)?;
            }
            if let Some(parent) = self.heap().instance_graph_node(class)?.superclass {
                self.step()?;
                push(&mut work, parent)?;
            }
        }
        Ok(false)
    }
    pub fn is_assignable(
        &mut self,
        mut source: ReferenceType,
        mut target: ReferenceType,
    ) -> Result<bool, TypeAccessError> {
        self.unchanged()?;
        let universe = self.universe()?;
        self.validate_reference_type(source, universe)?;
        self.validate_reference_type(target, universe)?;
        loop {
            self.step()?; // type pair, including identical supplied identities
            match (source, target) {
                (ReferenceType::Class(source), ReferenceType::Class(target)) => {
                    return self.nominal_assignment(source, target);
                }
                (ReferenceType::Array { .. }, ReferenceType::Class(target)) => {
                    return Ok([universe.object, universe.cloneable, universe.serializable]
                        .contains(&target));
                }
                (ReferenceType::Class(_), ReferenceType::Array { .. }) => return Ok(false),
                (
                    ReferenceType::Array {
                        dimensions: sd,
                        element: se,
                        defining_loader: sl,
                    },
                    ReferenceType::Array {
                        dimensions: td,
                        element: te,
                        defining_loader: tl,
                    },
                ) => {
                    if sd == 1 && matches!(se, ArrayElement::Primitive(_))
                        || td == 1 && matches!(te, ArrayElement::Primitive(_))
                    {
                        return Ok(sd == 1 && td == 1 && se == te);
                    }
                    // Both components are reference types. Reduce ranks without
                    // building a recursive descriptor/type tree or host recursion.
                    source = component(sd, se, sl)?;
                    target = component(td, te, tl)?;
                }
            }
        }
    }
    pub fn check_cast(
        &mut self,
        origin: ClassId,
        index: u16,
        reference: Reference,
    ) -> Result<Reference, TypeAccessError> {
        let name = self.type_name(origin, index)?;
        if reference.is_null() {
            return Ok(reference);
        }
        let source = self
            .reference_type(reference)?
            .ok_or(TypeAccessError::InvalidType(
                "nonnull source has no runtime type",
            ))?;
        let target = self.resolve_type_name_value(origin, name)?;
        if self.is_assignable(source, target)? {
            Ok(reference)
        } else {
            Err(TypeAccessError::Java(TypeJavaError::ClassCast))
        }
    }
    pub fn instance_of(
        &mut self,
        origin: ClassId,
        index: u16,
        reference: Reference,
    ) -> Result<bool, TypeAccessError> {
        let name = self.type_name(origin, index)?;
        if reference.is_null() {
            return Ok(false);
        }
        let source = self
            .reference_type(reference)?
            .ok_or(TypeAccessError::InvalidType(
                "nonnull source has no runtime type",
            ))?;
        let target = self.resolve_type_name_value(origin, name)?;
        self.is_assignable(source, target)
    }
}
fn component(
    dimensions: u8,
    element: ArrayElement,
    defining_loader: LoaderId,
) -> Result<ReferenceType, TypeAccessError> {
    if dimensions > 1 {
        Ok(ReferenceType::Array {
            dimensions: dimensions - 1,
            element,
            defining_loader,
        })
    } else if let ArrayElement::Class(class) = element {
        Ok(ReferenceType::Class(class))
    } else {
        Err(TypeAccessError::InvalidType(
            "primitive component is not a reference type",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HeapOptions, HierarchyOptions, InstanceOptions, RegistryOptions, SameLoaderResolver,
        build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;
    fn fixture() -> (DefinitionRegistry, ManagedHeap, ReferenceTypeUniverse) {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut classes = Vec::new();
        for (name, flags, parent) in [
            ("java/lang/Object", 0x21_u16, 0_u16),
            ("java/lang/Cloneable", 0x601, 4),
            ("java/io/Serializable", 0x601, 4),
        ] {
            let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 5];
            for (index, text) in [(1, name), (3, "java/lang/Object")] {
                bytes.push(1);
                bytes.extend((text.len() as u16).to_be_bytes());
                bytes.extend(text.bytes());
                bytes.extend([7, 0, index]);
            }
            for word in [flags, 2, parent, 0, 0, 0, 0] {
                bytes.extend(word.to_be_bytes());
            }
            classes.push(
                registry
                    .define(
                        loader,
                        &name.encode_utf16().collect::<Vec<_>>(),
                        &bytes,
                        ParseOptions::default(),
                    )
                    .unwrap(),
            );
        }
        let universe = ReferenceTypeUniverse {
            bootstrap_loader: loader,
            object: classes[0],
            cloneable: classes[1],
            serializable: classes[2],
        };
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
        heap.bind_reference_types(&registry, universe).unwrap();
        (registry, heap, universe)
    }
    #[test]
    fn temporary_reservation_overflow_preserves_existing_entries() {
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let mut work = vec![class];
        let mut seen = HashSet::from([class]);
        assert_eq!(reserve_work(&mut work, usize::MAX), Err(allocation()));
        assert_eq!(reserve_seen(&mut seen, usize::MAX), Err(allocation()));
        assert_eq!(work, [class]);
        assert!(seen.contains(&class));
    }
    #[test]
    fn forged_handle_components_and_nominal_indices_are_checked_before_identity() {
        let (registry, mut heap, u) = fixture();
        let array = heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let (id, index, generation) = array.heap_ref().unwrap().components();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            crate::FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            ctx.reference_type(Reference::issued(id, usize::MAX, generation)),
            Err(TypeAccessError::Heap(HeapError::MissingEntry {
                index: usize::MAX
            }))
        );
        assert_eq!(
            ctx.reference_type(Reference::issued(id, index, generation + 1)),
            Err(TypeAccessError::Heap(HeapError::StaleGeneration {
                expected: generation,
                actual: generation + 1
            }))
        );
        let forged = ClassId {
            registry: u.object.registry,
            index: usize::MAX,
        };
        assert!(matches!(
            ctx.is_assignable(ReferenceType::Class(forged), ReferenceType::Class(forged)),
            Err(TypeAccessError::Context(FieldAccessError::Registry(_)))
        ));
        assert_eq!(ctx.subclass(u.cloneable, u.object), Ok(false));
        assert_eq!(
            ctx.is_assignable(
                ReferenceType::Class(u.cloneable),
                ReferenceType::Class(u.object)
            ),
            Ok(true)
        );
    }
    #[test]
    fn wrapped_errors_keep_sources_and_corrupt_array_names_are_structured() {
        let wrapped =
            TypeAccessError::from(FieldAccessError::Java(crate::FieldJavaError::IllegalAccess));
        assert!(wrapped.source().is_some());
        assert!(matches!(wrapped, TypeAccessError::Context(_)));
        assert!(
            TypeAccessError::Java(TypeJavaError::ClassCast)
                .source()
                .is_none()
        );
        let (registry, mut heap, u) = fixture();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            crate::FieldResolutionOptions::default(),
        )
        .unwrap();
        for name in [
            &[91_u16][..],
            &[91, 86],
            &[91, 76, 59],
            &[91, 76, 47, 65, 59],
            &[91, 76, 65, 46, 66, 59],
        ] {
            assert!(matches!(
                ctx.resolve_type_name_value(u.object, name),
                Err(TypeAccessError::InvalidType(_))
            ));
        }
        let oversized = vec![91_u16; 256];
        assert!(matches!(
            ctx.resolve_type_name_value(u.object, &oversized),
            Err(TypeAccessError::InvalidType(_))
        ));
    }
}
