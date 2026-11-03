//! Bounded class-method resolution and package-aware virtual selection.
use crate::{
    ClassId, FieldAccessError, FieldResolutionContext, HeapError, InstanceError, LoaderId,
    MethodId, Reference, RegistryError,
};
use crabspot_classfile::{
    Constant, JavaString,
    descriptor::{FieldType, parse_method_descriptor},
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodJavaError {
    NoSuchMethod,
    IllegalAccess,
    IncompatibleClassChange,
    NullPointer,
    AbstractMethod,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MethodAccessError {
    Java(MethodJavaError),
    Context(FieldAccessError),
    InvalidConstant {
        origin: ClassId,
        index: u16,
    },
    UnsupportedDescriptor,
    UnsupportedSpecialMethod,
    UnsupportedSignaturePolymorphic {
        method: MethodId,
    },
    /// An eligible declaration witness, not a maximally-specific selected target.
    UnsupportedInterfaceMethods {
        owner: ClassId,
    },
    UnsupportedNestAccess {
        origin: ClassId,
        owner: ClassId,
    },
    InvalidOverride {
        method: MethodId,
        overridden: MethodId,
    },
}
impl fmt::Display for MethodAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Java(error) => write!(f, "virtual method access failed: {error:?}"),
            Self::Context(error) => write!(f, "virtual method context failed: {error}"),
            Self::InvalidConstant { origin, index } => write!(
                f,
                "constant #{index} in {origin:?} is not a valid Methodref"
            ),
            Self::UnsupportedDescriptor => f.write_str(
                "virtual body calls require primitive parameters and primitive/void results",
            ),
            Self::UnsupportedSpecialMethod => {
                f.write_str("initialization methods are excluded from ordinary virtual body calls")
            }
            Self::UnsupportedSignaturePolymorphic { method } => write!(
                f,
                "signature-polymorphic method {method:?} requires unsupported invocation services"
            ),
            Self::UnsupportedInterfaceMethods { owner } => write!(
                f,
                "matching interface declaration in {owner:?} requires unsupported interface method resolution"
            ),
            Self::UnsupportedNestAccess { origin, owner } => write!(
                f,
                "private method access from {origin:?} to {owner:?} requires unsupported nest checks"
            ),
            Self::InvalidOverride { method, overridden } => write!(
                f,
                "method {method:?} overrides final declaration {overridden:?}"
            ),
        }
    }
}
impl std::error::Error for MethodAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Context(error) = self {
            Some(error)
        } else {
            None
        }
    }
}
impl From<FieldAccessError> for MethodAccessError {
    fn from(error: FieldAccessError) -> Self {
        Self::Context(error)
    }
}
impl From<RegistryError> for MethodAccessError {
    fn from(error: RegistryError) -> Self {
        Self::Context(FieldAccessError::Registry(error))
    }
}
impl From<InstanceError> for MethodAccessError {
    fn from(error: InstanceError) -> Self {
        Self::Context(FieldAccessError::Instance(error))
    }
}
fn allocation() -> MethodAccessError {
    InstanceError::Heap(HeapError::AllocationFailure {
        resource: "virtual method lookup metadata",
    })
    .into()
}
fn reserve<T>(values: &mut Vec<T>, amount: usize) -> Result<(), MethodAccessError> {
    values.try_reserve(amount).map_err(|_| allocation())
}
fn reserve_set<T: Eq + std::hash::Hash>(
    set: &mut HashSet<T>,
    amount: usize,
) -> Result<(), MethodAccessError> {
    set.try_reserve(amount).map_err(|_| allocation())
}
fn reserve_map<K: Eq + std::hash::Hash, V>(
    map: &mut HashMap<K, V>,
    amount: usize,
) -> Result<(), MethodAccessError> {
    map.try_reserve(amount).map_err(|_| allocation())
}
type Package<'a> = (LoaderId, &'a [u16]);
struct Symbol<'a> {
    owner: &'a JavaString,
    name: &'a JavaString,
    descriptor: &'a JavaString,
}
struct Resolved {
    owner: ClassId,
    method: MethodId,
    protected_receiver: Option<ClassId>,
}

impl<'a> FieldResolutionContext<'a> {
    fn method_symbol(&self, origin: ClassId, index: u16) -> Result<Symbol<'a>, MethodAccessError> {
        let invalid = || MethodAccessError::InvalidConstant { origin, index };
        let pool = &self.registry().class(origin)?.constant_pool;
        let Constant::Methodref {
            class_index,
            name_and_type_index,
        } = pool.get(index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let owner = pool.class_name(*class_index).map_err(|_| invalid())?;
        if owner.units().first() == Some(&91) {
            return Err(FieldAccessError::UnsupportedArrayOwner.into());
        }
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = pool.get(*name_and_type_index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let name = pool.utf8(*name_index).map_err(|_| invalid())?;
        let descriptor = pool.utf8(*descriptor_index).map_err(|_| invalid())?;
        if name.equals_ascii("<init>") || name.equals_ascii("<clinit>") {
            return Err(MethodAccessError::UnsupportedSpecialMethod);
        }
        let parsed = parse_method_descriptor(descriptor.units()).map_err(|_| invalid())?;
        if parsed
            .parameters
            .iter()
            .any(|field| matches!(field, FieldType::Object(_) | FieldType::Array { .. }))
            || parsed.return_type.as_ref().is_some_and(|field| {
                matches!(field, FieldType::Object(_) | FieldType::Array { .. })
            })
        {
            return Err(MethodAccessError::UnsupportedDescriptor);
        }
        Ok(Symbol {
            owner,
            name,
            descriptor,
        })
    }
    fn runtime_package(&self, class: ClassId) -> Result<Package<'a>, MethodAccessError> {
        let registry = self.registry();
        let name = registry
            .class(class)?
            .name()
            .map_err(RegistryError::Parse)?
            .units();
        let end = name.iter().rposition(|unit| *unit == 47).unwrap_or(0);
        Ok((registry.defining_loader(class)?, &name[..end]))
    }
    fn is_signature_polymorphic(&self, method: MethodId) -> Result<bool, MethodAccessError> {
        let registry = self.registry();
        let class = registry.class(method.owner())?;
        let name = class.name().map_err(RegistryError::Parse)?;
        if !(name.equals_ascii("java/lang/invoke/MethodHandle")
            || name.equals_ascii("java/lang/invoke/VarHandle"))
        {
            return Ok(false);
        }
        let member = registry.method(method)?;
        if member.access_flags & 0x180 != 0x180 {
            return Ok(false);
        }
        let descriptor = class
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?;
        let parsed = parse_method_descriptor(descriptor.units()).map_err(RegistryError::Parse)?;
        Ok(
            matches!(parsed.parameters.as_slice(), [FieldType::Array { dimensions: 1, component }] if matches!(component.as_ref(), FieldType::Object(name) if name.equals_ascii("java/lang/Object"))),
        )
    }
    fn declared_method(
        &mut self,
        class: ClassId,
        symbol: &Symbol<'_>,
        polymorphic: bool,
    ) -> Result<Option<MethodId>, MethodAccessError> {
        let registry = self.registry();
        let definition = registry.class(class)?;
        let mut matching_name = 0usize;
        let mut named = None;
        let mut exact = None;
        for (index, member) in definition.methods.iter().enumerate() {
            self.step()?;
            if definition
                .constant_pool
                .utf8(member.name_index)
                .map_err(RegistryError::Parse)?
                != symbol.name
            {
                continue;
            }
            matching_name = matching_name
                .checked_add(1)
                .ok_or(FieldAccessError::InvalidState("method name count overflow"))?;
            let method = MethodId {
                owner: class,
                index,
            };
            named = Some(method);
            if definition
                .constant_pool
                .utf8(member.descriptor_index)
                .map_err(RegistryError::Parse)?
                == symbol.descriptor
                && exact.is_none()
            {
                exact = Some(method);
            }
        }
        if polymorphic && matching_name == 1 {
            if let Some(method) = named {
                if self.is_signature_polymorphic(method)? {
                    return Err(MethodAccessError::UnsupportedSignaturePolymorphic { method });
                }
            }
        }
        Ok(exact)
    }
    fn interface_fallback(
        &mut self,
        owner: ClassId,
        symbol: &Symbol<'_>,
    ) -> Result<MethodId, MethodAccessError> {
        let mut work = Vec::new();
        let mut current = Some(owner);
        for _ in 0..self.heap().retained_layout_classes() {
            let Some(class) = current else {
                break;
            };
            self.step()?;
            let node = self.heap().instance_graph_node(class)?;
            reserve(&mut work, node.interfaces.len())?;
            work.extend(node.interfaces.iter().rev().copied());
            current = node.superclass;
        }
        if current.is_some() {
            return Err(FieldAccessError::InvalidState("cyclic method owner ancestry").into());
        }
        let mut visited = HashSet::new();
        while let Some(interface) = work.pop() {
            if visited.contains(&interface) {
                continue;
            }
            self.step()?;
            reserve_set(&mut visited, 1)?;
            visited.insert(interface);
            if let Some(method) = self.declared_method(interface, symbol, false)? {
                if self.registry().method(method)?.access_flags & 0xa == 0 {
                    return Err(MethodAccessError::UnsupportedInterfaceMethods {
                        owner: interface,
                    });
                }
            }
            let node = self.heap().instance_graph_node(interface)?;
            reserve(&mut work, node.interfaces.len())?;
            work.extend(node.interfaces.iter().rev().copied());
        }
        Err(MethodAccessError::Java(MethodJavaError::NoSuchMethod))
    }
    fn resolve_method(
        &mut self,
        origin: ClassId,
        symbol: &Symbol<'a>,
    ) -> Result<Resolved, MethodAccessError> {
        let owner = self.resolve_owner(origin, symbol.owner)?;
        if self.registry().class(owner)?.access_flags & 1 == 0
            && !self.same_package(origin, owner)?
        {
            return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
        }
        if self.heap().instance_graph_node(owner)?.flags & 0x200 != 0 {
            return Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange,
            ));
        }
        let mut current = Some(owner);
        let mut found = None;
        for _ in 0..self.heap().retained_layout_classes() {
            let Some(class) = current else {
                break;
            };
            self.step()?;
            if let Some(method) = self.declared_method(class, symbol, true)? {
                found = Some(method);
                break;
            }
            current = self.heap().instance_graph_node(class)?.superclass;
        }
        let method = match found {
            Some(method) => method,
            None if current.is_none() => self.interface_fallback(owner, symbol)?,
            None => {
                return Err(
                    FieldAccessError::InvalidState("cyclic method resolution ancestry").into(),
                );
            }
        };
        let flags = self.registry().method(method)?.access_flags;
        let declaring = method.owner();
        let mut protected_receiver = None;
        if flags & 1 != 0 { /* public */
        } else if flags & 2 != 0 {
            if origin != declaring {
                return Err(MethodAccessError::UnsupportedNestAccess {
                    origin,
                    owner: declaring,
                });
            }
        } else if flags & 4 != 0 {
            if !self.same_package(origin, declaring)? {
                if !self.subclass(origin, declaring)? {
                    return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
                }
                if flags & 8 == 0 {
                    if !(self.subclass(origin, owner)? || self.subclass(owner, origin)?) {
                        return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
                    }
                    protected_receiver = Some(origin);
                }
            }
        } else if !self.same_package(origin, declaring)? {
            return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
        }
        if flags & 8 != 0 {
            return Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange,
            ));
        }
        Ok(Resolved {
            owner,
            method,
            protected_receiver,
        })
    }
    fn receiver_chain(&mut self, mut class: ClassId) -> Result<Vec<ClassId>, MethodAccessError> {
        let mut chain = Vec::new();
        for _ in 0..self.heap().retained_layout_classes() {
            self.step()?;
            reserve(&mut chain, 1)?;
            chain.push(class);
            match self.heap().instance_graph_node(class)?.superclass {
                Some(parent) => class = parent,
                None => return Ok(chain),
            }
        }
        Err(FieldAccessError::InvalidState("cyclic virtual receiver ancestry").into())
    }
    fn select_override(
        &mut self,
        resolved: MethodId,
        actual: ClassId,
        symbol: &Symbol<'_>,
    ) -> Result<MethodId, MethodAccessError> {
        let chain = self.receiver_chain(actual)?;
        let private = self.registry().method(resolved)?.access_flags & 2 != 0;
        let mut selected = resolved;
        let mut reached = false;
        let mut open = self.registry().method(resolved)?.access_flags & 5 != 0;
        let mut packages = HashSet::new();
        reserve_set(&mut packages, 1)?;
        packages.insert(self.runtime_package(resolved.owner())?);
        let mut public_final = None;
        let mut package_finals = HashMap::new();
        for class in chain.into_iter().rev() {
            let Some(method) = self.declared_method(class, symbol, false)? else {
                continue;
            };
            if method == resolved {
                reached = true;
            }
            let flags = self.registry().method(method)?.access_flags;
            if flags & 0xa != 0 {
                continue;
            } // static/private declarations do not override
            self.step()?; // matching-declaration override/finality relation work
            let package = self.runtime_package(class)?;
            if let Some(overridden) = public_final.or_else(|| package_finals.get(&package).copied())
            {
                return Err(MethodAccessError::InvalidOverride { method, overridden });
            }
            if flags & 16 != 0 {
                if flags & 5 != 0 {
                    public_final = Some(method);
                } else {
                    reserve_map(&mut package_finals, 1)?;
                    package_finals.insert(package, method);
                }
            }
            if private || !reached || method == resolved {
                continue;
            }
            if open || packages.contains(&package) {
                selected = method;
                if !packages.contains(&package) {
                    reserve_set(&mut packages, 1)?;
                    packages.insert(package);
                }
                if flags & 5 != 0 {
                    open = true;
                }
            }
        }
        if !reached {
            return Err(FieldAccessError::InvalidState(
                "resolved method is outside receiver ancestry",
            )
            .into());
        }
        Ok(selected)
    }
    /// Select one class body using the context's existing graph, bindings and budgets.
    /// This does not prepare Code, execute native bodies or establish verification.
    pub fn select_virtual(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<MethodId, MethodAccessError> {
        self.unchanged()?;
        self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        let symbol = self.method_symbol(origin, index)?;
        let resolved = self.resolve_method(origin, &symbol)?;
        if receiver.is_null() {
            return Err(MethodAccessError::Java(MethodJavaError::NullPointer));
        }
        let actual = self.heap().instance_class(receiver)?;
        if !self.subclass(actual, resolved.owner)? {
            return Err(FieldAccessError::InvalidReceiver {
                expected: resolved.owner,
                actual,
            }
            .into());
        }
        if let Some(expected) = resolved.protected_receiver {
            if !self.subclass(actual, expected)? {
                return Err(FieldAccessError::InvalidReceiver { expected, actual }.into());
            }
        }
        let selected = self.select_override(resolved.method, actual, &symbol)?;
        if self.registry().method(selected)?.access_flags & 0x400 != 0 {
            return Err(MethodAccessError::Java(MethodJavaError::AbstractMethod));
        }
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FieldResolutionOptions, HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap,
        RegistryOptions, SameLoaderResolver, build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;

    fn fixture() -> (crate::DefinitionRegistry, ClassId) {
        let mut registry = crate::DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 7];
        for (tag, data) in [
            (1, b"java/lang/Object".as_slice()),
            (7, &[0, 1]),
            (1, b"m".as_slice()),
            (1, b"()I".as_slice()),
            (12, &[0, 3, 0, 4]),
            (10, &[0, 2, 0, 5]),
        ] {
            bytes.push(tag);
            if tag == 1 {
                bytes.extend((data.len() as u16).to_be_bytes());
            }
            bytes.extend(data);
        }
        for word in [0x21_u16, 2, 0, 0, 0, 1, 0x101, 3, 4, 0, 0] {
            bytes.extend(word.to_be_bytes());
        }
        let class = registry
            .define(
                loader,
                &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                &bytes,
                ParseOptions::default(),
            )
            .unwrap();
        (registry, class)
    }
    #[test]
    fn deterministic_temporary_reservation_overflows_preserve_existing_values() {
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let loader = LoaderId {
            registry: 1,
            index: 0,
        };
        let mut values = vec![class];
        let mut set = HashSet::from([class]);
        let mut map = HashMap::from([(
            (loader, &[65_u16][..]),
            MethodId {
                owner: class,
                index: 0,
            },
        )]);
        for result in [
            reserve(&mut values, usize::MAX),
            reserve_set(&mut set, usize::MAX),
            reserve_map(&mut map, usize::MAX),
        ] {
            assert_eq!(result.unwrap_err(), allocation());
        }
        assert_eq!(values, [class]);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&class));
        assert_eq!(map.len(), 1);
    }
    #[test]
    fn private_forged_method_and_origin_handles_remain_checked() {
        let (registry, class) = fixture();
        let plan = build_hierarchy(
            &registry,
            class,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        let receiver = heap.allocate_instance(class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let forged_origin = ClassId {
            registry: class.registry,
            index: usize::MAX,
        };
        assert!(matches!(
            context.select_virtual(forged_origin, 6, receiver),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        let symbol = context.method_symbol(class, 6).unwrap();
        let forged = MethodId {
            owner: class,
            index: usize::MAX,
        };
        assert!(matches!(
            context.select_override(forged, class, &symbol),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        assert_eq!(
            context.select_virtual(class, 6, receiver).unwrap(),
            MethodId {
                owner: class,
                index: 0
            }
        );
    }
    #[test]
    fn context_java_errors_are_not_implicitly_reclassified_and_keep_sources() {
        let inner = FieldAccessError::Java(crate::FieldJavaError::IllegalAccess);
        let wrapped = MethodAccessError::from(inner.clone());
        assert_eq!(wrapped, MethodAccessError::Context(inner));
        assert!(wrapped.source().is_some());
        let allocation = allocation();
        assert!(allocation.source().unwrap().source().is_some());
        assert!(allocation.to_string().contains("context"));
        assert!(
            MethodAccessError::Java(MethodJavaError::NoSuchMethod)
                .source()
                .is_none()
        );
    }
}
