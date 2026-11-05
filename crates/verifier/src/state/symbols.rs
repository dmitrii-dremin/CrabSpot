use super::*;
use crabspot_classfile::{Constant, Member};
use crabspot_runtime::{ClassId, LoaderId};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum Primitive {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum Leaf {
    Primitive(Primitive),
    Class(usize),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum Ty {
    Top,
    WideTail,
    Int,
    Long,
    Float,
    Double,
    Null,
    Class(usize),
    Array(u8, Leaf),
    Uninit(u32),
    This,
}
impl Ty {
    pub fn width(self) -> usize {
        if matches!(self, Self::Long | Self::Double) {
            2
        } else {
            1
        }
    }
    pub fn generic(self) -> bool {
        matches!(
            self,
            Self::Null | Self::Class(_) | Self::Array(..) | Self::Uninit(_) | Self::This
        )
    }
    pub fn ordinary(self) -> bool {
        matches!(self, Self::Null | Self::Class(_) | Self::Array(..))
    }
}
#[derive(Debug)]
struct Symbol<'a> {
    loader: LoaderId,
    name: &'a [u16],
    source: VerificationSymbolSource,
}
#[derive(Debug)]
pub(super) struct Signature {
    pub parameters: Vec<Ty>,
    pub result: Option<Ty>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct Key {
    owner: ClassId,
    index: u16,
    role: u8,
}
#[derive(Clone, Copy, Debug)]
enum Cached {
    Type(Ty),
    Signature(usize),
    Name(u8),
}
pub(super) struct Proof<'e, 'a> {
    pub env: &'e VerificationEnvironment<'a>,
    pub class: ClassId,
    pub options: TypeVerificationOptions,
    pub usage: TypeVerificationUsage,
    pub location: CodeValidationLocation,
    symbols: Vec<Symbol<'a>>,
    intern: HashMap<(LoaderId, Spelling<'a>), usize>,
    cache: HashMap<Key, Cached>,
    resolved: HashMap<usize, ClassId>,
    pub signatures: Vec<Signature>,
    pub arena: Vec<super::frames::Prefix>,
    pub maps: Vec<super::frames::Map>,
}
impl<'e, 'a> Proof<'e, 'a> {
    pub fn new(
        env: &'e VerificationEnvironment<'a>,
        class: ClassId,
        options: TypeVerificationOptions,
        code: CodeValidationUsage,
    ) -> Self {
        Self {
            env,
            class,
            options,
            usage: TypeVerificationUsage {
                code,
                ..Default::default()
            },
            location: CodeValidationLocation::Method,
            symbols: Vec::new(),
            intern: HashMap::new(),
            cache: HashMap::new(),
            resolved: HashMap::new(),
            signatures: Vec::new(),
            arena: Vec::new(),
            maps: Vec::new(),
        }
    }
    pub fn charge(&mut self, symbols: u64, cells: u64, edges: u64, utf: u64, work: u64) -> T<()> {
        use TypeVerificationResource::*;
        let u = self.usage;
        let o = self.options;
        let a = [
            (u.type_symbols, symbols, o.max_type_symbols, TypeSymbols),
            (u.state_cells, cells, o.max_state_cells, StateCells),
            (u.edge_checks, edges, o.max_edge_checks, EdgeChecks),
            (u.utf16_units, utf, o.max_utf16_units, Utf16Units),
            (u.work_units, work, o.max_work_units, WorkUnits),
        ];
        let mut t = [0; 5];
        for (dst, (old, n, limit, resource)) in t.iter_mut().zip(a) {
            *dst = old
                .checked_add(n)
                .ok_or(TypeVerificationErrorKind::AccountingOverflow { resource })?;
            if *dst > limit {
                return Err(TypeVerificationErrorKind::ResourceLimit {
                    resource,
                    limit,
                    required: *dst,
                });
            }
        }
        self.usage.type_symbols = t[0];
        self.usage.state_cells = t[1];
        self.usage.edge_checks = t[2];
        self.usage.utf16_units = t[3];
        self.usage.work_units = t[4];
        Ok(())
    }
    pub fn work(&mut self, n: usize) -> T<()> {
        let n = u64::try_from(n).map_err(|_| TypeVerificationErrorKind::AccountingOverflow {
            resource: TypeVerificationResource::WorkUnits,
        })?;
        self.charge(0, 0, 0, 0, n)
    }
    pub fn cells(&mut self, n: usize) -> T<()> {
        let n = u64::try_from(n).map_err(|_| TypeVerificationErrorKind::AccountingOverflow {
            resource: TypeVerificationResource::StateCells,
        })?;
        self.charge(0, n, 0, 0, n)
    }
    pub fn edge(&mut self) -> T<()> {
        self.charge(0, 0, 1, 0, 1)
    }
    pub fn record(&mut self) -> T<()> {
        self.charge(1, 0, 0, 0, 1)
    }
    pub fn utf_work(&mut self, n: usize) -> T<()> {
        let n = u64::try_from(n).map_err(|_| TypeVerificationErrorKind::AccountingOverflow {
            resource: TypeVerificationResource::Utf16Units,
        })?;
        self.charge(0, 0, 0, n, n)
    }
    pub fn cp(&mut self, owner: ClassId, index: u16) -> T<&'a Constant> {
        self.work(1)?;
        self.env
            .registry
            .class(owner)?
            .constant_pool
            .get(index)
            .map_err(|_| bad("invalid checked constant link"))
    }
    pub fn utf(&mut self, owner: ClassId, index: u16) -> T<&'a [u16]> {
        match self.cp(owner, index)? {
            Constant::Utf8(s) => Ok(s.units()),
            _ => Err(bad("expected checked Utf8")),
        }
    }
    fn cached(&mut self, key: Key) -> T<Option<Cached>> {
        self.work(1)?;
        Ok(self.cache.get(&key).copied())
    }
    fn cache(&mut self, key: Key, value: Cached) -> T<()> {
        self.record()?;
        self.cache
            .try_reserve(1)
            .map_err(|_| TypeVerificationErrorKind::AllocationFailure {
                resource: "type source cache",
            })?;
        self.cache.insert(key, value);
        Ok(())
    }
    fn symbol(
        &mut self,
        loader: LoaderId,
        name: &'a [u16],
        source: VerificationSymbolSource,
    ) -> T<usize> {
        self.utf_work(name.len())?;
        let spelling = Spelling::new(name, &self.env.spelling_hash);
        if let Some(id) = self.intern.get(&(loader, spelling)) {
            return Ok(*id);
        }
        self.record()?;
        reserve(&mut self.symbols, 1, "nominal symbols")?;
        self.intern
            .try_reserve(1)
            .map_err(|_| TypeVerificationErrorKind::AllocationFailure {
                resource: "symbol interner",
            })?;
        let id = self.symbols.len();
        self.symbols.push(Symbol {
            loader,
            name,
            source,
        });
        self.intern.insert((loader, spelling), id);
        Ok(id)
    }
    pub fn definition_ty(&mut self, class: ClassId) -> T<Ty> {
        self.work(1)?;
        let node = self
            .env
            .nodes
            .get(&class)
            .ok_or(TypeVerificationErrorKind::UnretainedClass { class })?;
        Ok(Ty::Class(self.symbol(
            node.loader,
            node.name,
            VerificationSymbolSource::Definition(class),
        )?))
    }
    pub fn core_ty(&mut self, role: VerificationCoreRole) -> T<Ty> {
        let (_, class, _) = self
            .env
            .core
            .roles()
            .into_iter()
            .find(|(r, _, _)| *r == role)
            .ok_or(bad("unknown core role"))?;
        let node = self
            .env
            .nodes
            .get(&class)
            .ok_or(TypeVerificationErrorKind::UnretainedClass { class })?;
        Ok(Ty::Class(self.symbol(
            node.loader,
            node.name,
            VerificationSymbolSource::Core(role),
        )?))
    }
    fn field_at(&mut self, owner: ClassId, index: u16, units: &'a [u16], p: &mut usize) -> T<Ty> {
        let mut rank = 0_u16;
        while units.get(*p) == Some(&91) {
            rank = rank.checked_add(1).ok_or(bad("array rank overflow"))?;
            if rank > 255 {
                return Err(bad("array rank exceeds255"));
            }
            *p = p
                .checked_add(1)
                .ok_or(bad("descriptor position overflow"))?;
        }
        let tag = *units.get(*p).ok_or(bad("truncated descriptor"))?;
        *p = p
            .checked_add(1)
            .ok_or(bad("descriptor position overflow"))?;
        let (ty, leaf) = match tag {
            90 => (Ty::Int, Leaf::Primitive(Primitive::Boolean)),
            66 => (Ty::Int, Leaf::Primitive(Primitive::Byte)),
            67 => (Ty::Int, Leaf::Primitive(Primitive::Char)),
            83 => (Ty::Int, Leaf::Primitive(Primitive::Short)),
            73 => (Ty::Int, Leaf::Primitive(Primitive::Int)),
            74 => (Ty::Long, Leaf::Primitive(Primitive::Long)),
            70 => (Ty::Float, Leaf::Primitive(Primitive::Float)),
            68 => (Ty::Double, Leaf::Primitive(Primitive::Double)),
            76 => {
                let start = *p;
                while units.get(*p).is_some_and(|u| *u != 59) {
                    *p = p
                        .checked_add(1)
                        .ok_or(bad("descriptor position overflow"))?;
                }
                let name = units.get(start..*p).ok_or(bad("descriptor span invalid"))?;
                if units.get(*p) != Some(&59) {
                    return Err(bad("unterminated object descriptor"));
                }
                let source = VerificationSymbolSource::Utf8 {
                    class: owner,
                    index,
                    start: u16::try_from(start).map_err(|_| bad("descriptor span overflow"))?,
                    end: u16::try_from(*p).map_err(|_| bad("descriptor span overflow"))?,
                };
                let id = self.symbol(self.env.registry.defining_loader(owner)?, name, source)?;
                *p = p
                    .checked_add(1)
                    .ok_or(bad("descriptor position overflow"))?;
                (Ty::Class(id), Leaf::Class(id))
            }
            _ => return Err(bad("invalid descriptor type")),
        };
        Ok(if rank == 0 {
            ty
        } else {
            Ty::Array(
                u8::try_from(rank).map_err(|_| bad("array rank overflow"))?,
                leaf,
            )
        })
    }
    pub fn field_type(&mut self, owner: ClassId, index: u16) -> T<Ty> {
        let units = self.utf(owner, index)?;
        let key = Key {
            owner,
            index,
            role: 0,
        };
        if let Some(Cached::Type(ty)) = self.cached(key)? {
            return Ok(ty);
        }
        self.utf_work(units.len())?;
        let mut p = 0;
        let ty = self.field_at(owner, index, units, &mut p)?;
        if p != units.len() {
            return Err(bad("trailing field descriptor"));
        }
        self.cache(key, Cached::Type(ty))?;
        Ok(ty)
    }
    pub fn signature(&mut self, owner: ClassId, index: u16) -> T<usize> {
        let units = self.utf(owner, index)?;
        let key = Key {
            owner,
            index,
            role: 1,
        };
        if let Some(Cached::Signature(id)) = self.cached(key)? {
            return Ok(id);
        }
        self.utf_work(units.len())?;
        if units.first() != Some(&40) {
            return Err(bad("invalid method descriptor"));
        }
        let mut p = 1;
        let mut parameters = Vec::new();
        while units.get(p) != Some(&41) {
            let ty = self.field_at(owner, index, units, &mut p)?;
            self.cells(1)?;
            reserve(&mut parameters, 1, "compact signature")?;
            parameters.push(ty);
        }
        p = p
            .checked_add(1)
            .ok_or(bad("descriptor position overflow"))?;
        let result = if units.get(p) == Some(&86) {
            p += 1;
            None
        } else {
            Some(self.field_at(owner, index, units, &mut p)?)
        };
        if p != units.len() {
            return Err(bad("trailing method descriptor"));
        }
        self.record()?;
        reserve(&mut self.signatures, 1, "signature summaries")?;
        let id = self.signatures.len();
        self.signatures.push(Signature { parameters, result });
        self.cache(key, Cached::Signature(id))?;
        Ok(id)
    }
    pub fn name(&mut self, owner: ClassId, index: u16) -> T<(&'a [u16], u8)> {
        let units = self.utf(owner, index)?;
        let key = Key {
            owner,
            index,
            role: 2,
        };
        if let Some(Cached::Name(n)) = self.cached(key)? {
            return Ok((units, n));
        }
        self.utf_work(units.len())?;
        let n = if units.iter().copied().eq("<init>".bytes().map(u16::from)) {
            1
        } else if units.iter().copied().eq("<clinit>".bytes().map(u16::from)) {
            2
        } else {
            0
        };
        self.cache(key, Cached::Name(n))?;
        Ok((units, n))
    }
    pub fn class_ty(&mut self, owner: ClassId, index: u16) -> T<Ty> {
        let Constant::Class { name_index } = self.cp(owner, index)? else {
            return Err(bad("expected Class"));
        };
        let index = *name_index;
        let units = self.utf(owner, index)?;
        let key = Key {
            owner,
            index,
            role: 3,
        };
        if let Some(Cached::Type(ty)) = self.cached(key)? {
            return Ok(ty);
        }
        self.utf_work(units.len())?;
        let ty = if units.first() == Some(&91) {
            let mut p = 0;
            let ty = self.field_at(owner, index, units, &mut p)?;
            if p != units.len() {
                return Err(bad("invalid array Class"));
            }
            ty
        } else {
            Ty::Class(self.symbol(
                self.env.registry.defining_loader(owner)?,
                units,
                VerificationSymbolSource::Utf8 {
                    class: owner,
                    index,
                    start: 0,
                    end: u16::try_from(units.len()).map_err(|_| bad("class name span overflow"))?,
                },
            )?)
        };
        self.cache(key, Cached::Type(ty))?;
        Ok(ty)
    }
    pub fn resolve(&mut self, id: usize) -> T<ClassId> {
        self.work(1)?;
        if let Some(class) = self.resolved.get(&id) {
            return Ok(*class);
        }
        let symbol = self
            .symbols
            .get(id)
            .ok_or(bad("corrupt symbolic type index"))?;
        let (loader, name, source) = (symbol.loader, symbol.name, symbol.source);
        let class = match source {
            VerificationSymbolSource::Definition(class) => class,
            VerificationSymbolSource::Core(role) => self
                .env
                .core
                .roles()
                .into_iter()
                .find(|(r, _, _)| *r == role)
                .map(|(_, class, _)| class)
                .ok_or(bad("unknown core role"))?,
            _ => {
                self.utf_work(name.len())?;
                *self
                    .env
                    .bindings
                    .get(&(loader, Spelling::new(name, &self.env.spelling_hash)))
                    .ok_or(TypeVerificationErrorKind::UnavailableFact {
                        initiating_loader: loader,
                        symbol: source,
                    })?
            }
        };
        if !self.env.nodes.contains_key(&class) {
            return Err(TypeVerificationErrorKind::UnretainedClass { class });
        }
        self.record()?;
        self.resolved
            .try_reserve(1)
            .map_err(|_| TypeVerificationErrorKind::AllocationFailure {
                resource: "resolved type cache",
            })?;
        self.resolved.insert(id, class);
        Ok(class)
    }
    pub fn symbol_name(&self, id: usize) -> T<&'a [u16]> {
        self.symbols
            .get(id)
            .map(|s| s.name)
            .ok_or(bad("corrupt symbolic name index"))
    }
    pub fn subclass(&mut self, mut from: ClassId, to: ClassId) -> T<bool> {
        for _ in 0..self.env.nodes.len() {
            self.work(1)?;
            if from == to {
                return Ok(true);
            }
            let node = self
                .env
                .nodes
                .get(&from)
                .ok_or(TypeVerificationErrorKind::UnretainedClass { class: from })?;
            match node.layout.direct_superclass() {
                Some(parent) => {
                    self.work(1)?;
                    from = parent;
                }
                None => return Ok(false),
            }
        }
        Err(bad("cyclic environment ancestry"))
    }
    pub fn assign(&mut self, from: Ty, to: Ty) -> T<bool> {
        self.work(1)?;
        self.assign_paid(from, to)
    }
    pub fn assign_paid(&mut self, mut from: Ty, mut to: Ty) -> T<bool> {
        loop {
            if from == to || to == Ty::Top {
                return Ok(true);
            }
            match (from, to) {
                (Ty::Null, Ty::Class(_) | Ty::Array(..)) => return Ok(true),
                (Ty::Class(source), Ty::Class(target)) => {
                    let target = self.resolve(target)?;
                    if target == self.env.core.object
                        || self
                            .env
                            .nodes
                            .get(&target)
                            .ok_or(TypeVerificationErrorKind::UnretainedClass { class: target })?
                            .metadata
                            .access_flags
                            & 0x200
                            != 0
                    {
                        return Ok(true);
                    }
                    let source = self.resolve(source)?;
                    return self.subclass(source, target);
                }
                (Ty::Array(..), Ty::Class(target)) => {
                    let target = self.resolve(target)?;
                    return Ok([
                        self.env.core.object,
                        self.env.core.cloneable,
                        self.env.core.serializable,
                    ]
                    .contains(&target));
                }
                (Ty::Array(fr, fl), Ty::Array(tr, tl)) => {
                    if fr == 0 || tr == 0 {
                        return Err(bad("corrupt zero-rank array type"));
                    }
                    if fr == 1 && matches!(fl, Leaf::Primitive(_))
                        || tr == 1 && matches!(tl, Leaf::Primitive(_))
                    {
                        return Ok(fr == 1 && tr == 1 && fl == tl);
                    }
                    from = if fr > 1 {
                        Ty::Array(fr - 1, fl)
                    } else {
                        match fl {
                            Leaf::Class(s) => Ty::Class(s),
                            _ => return Err(bad("invalid array component")),
                        }
                    };
                    to = if tr > 1 {
                        Ty::Array(tr - 1, tl)
                    } else {
                        match tl {
                            Leaf::Class(s) => Ty::Class(s),
                            _ => return Err(bad("invalid array component")),
                        }
                    };
                    self.work(1)?;
                }
                _ => return Ok(false),
            }
        }
    }
    pub fn require_assign(&mut self, from: Ty, to: Ty) -> T<()> {
        if self.assign(from, to)? {
            Ok(())
        } else {
            Err(bad("verification types are not assignable"))
        }
    }
    pub fn member_equal(
        &mut self,
        owner: ClassId,
        member: &Member,
        name: &[u16],
        descriptor: &[u16],
    ) -> T<bool> {
        self.work(1)?;
        let candidate_name = self.utf(owner, member.name_index)?;
        self.utf_work(candidate_name.len())?;
        self.utf_work(name.len())?;
        if candidate_name != name {
            return Ok(false);
        }
        let candidate_descriptor = self.utf(owner, member.descriptor_index)?;
        self.utf_work(candidate_descriptor.len())?;
        self.utf_work(descriptor.len())?;
        Ok(candidate_descriptor == descriptor)
    }
    pub fn final_override(&mut self, member: &Member) -> T<()> {
        let (name, special) = self.name(self.class, member.name_index)?;
        if special != 0 || member.access_flags & 10 != 0 {
            return Ok(());
        }
        let descriptor = self.utf(self.class, member.descriptor_index)?;
        let mut packages = std::collections::HashSet::new();
        packages
            .try_reserve(1)
            .map_err(|_| TypeVerificationErrorKind::AllocationFailure {
                resource: "reachable packages",
            })?;
        packages.insert(
            self.env
                .nodes
                .get(&self.class)
                .ok_or(TypeVerificationErrorKind::UnretainedClass { class: self.class })?
                .package,
        );
        let mut current = self
            .env
            .nodes
            .get(&self.class)
            .ok_or(TypeVerificationErrorKind::UnretainedClass { class: self.class })?
            .layout
            .direct_superclass();
        if current.is_some() {
            self.work(1)?;
        }
        for _ in 0..self.env.nodes.len() {
            let Some(class) = current else {
                return Ok(());
            };
            self.work(1)?;
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(TypeVerificationErrorKind::UnretainedClass { class })?;
            for (index, method) in node.metadata.methods.iter().enumerate() {
                if self.member_equal(class, method, name, descriptor)?
                    && method.access_flags & 10 == 0
                {
                    self.work(1)?;
                    if method.access_flags & 5 != 0 || packages.contains(&node.package) {
                        if method.access_flags & 16 != 0 {
                            return Err(TypeVerificationErrorKind::FinalOverride {
                                ancestor: class,
                                ancestor_method_index: index,
                            });
                        }
                        packages.try_reserve(1).map_err(|_| {
                            TypeVerificationErrorKind::AllocationFailure {
                                resource: "reachable packages",
                            }
                        })?;
                        packages.insert(node.package);
                    }
                    break;
                }
            }
            current = node.layout.direct_superclass();
            if current.is_some() {
                self.work(1)?;
            }
        }
        if current.is_some() {
            Err(bad("cyclic final-override ancestry"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Spec, World};
    use super::*;
    #[test]
    fn designated_object_accepts_unavailable_nominal_source_but_spelling_does_not() {
        let mut world = World::new(&[], &[]);
        let foreign = world.registry.create_loader().unwrap();
        let missing: Vec<_> = "not/Loaded".encode_utf16().collect();
        let object: Vec<_> = "java/lang/Object".encode_utf16().collect();
        let env = world.environment();
        let mut p = Proof::new(
            &env,
            world.core.object,
            TypeVerificationOptions::default(),
            CodeValidationUsage::default(),
        );
        let source = p
            .symbol(
                world.core.bootstrap_loader,
                &missing,
                VerificationSymbolSource::Utf8 {
                    class: world.core.object,
                    index: 0,
                    start: 0,
                    end: 0,
                },
            )
            .unwrap();
        let target = p.core_ty(VerificationCoreRole::Object).unwrap();
        assert!(p.assign(Ty::Class(source), target).unwrap());
        assert!(!p.resolved.contains_key(&source));
        let lookalike = p
            .symbol(
                foreign,
                &object,
                VerificationSymbolSource::Utf8 {
                    class: world.core.object,
                    index: 0,
                    start: 0,
                    end: 0,
                },
            )
            .unwrap();
        assert!(
            matches!(p.assign(Ty::Class(source),Ty::Class(lookalike)),Err(TypeVerificationErrorKind::UnavailableFact{initiating_loader,..}) if initiating_loader==foreign)
        );
    }
    #[test]
    fn initial_final_override_superclass_edge_has_its_own_inclusive_work_charge() {
        let world = World::new(
            &[Spec {
                name: "p/C",
                parent: Some("java/lang/Object"),
                interfaces: &[],
                flags: 0x21,
                fields: &[],
                methods: &[("run", "()V", 0x101)],
            }],
            &[],
        );
        let env = world.environment();
        let class = world.extra[0];
        let member = &world.registry.class(class).unwrap().methods[0];
        let mut p = Proof::new(
            &env,
            class,
            TypeVerificationOptions::default(),
            CodeValidationUsage::default(),
        );
        p.name(class, member.name_index).unwrap();
        // A cached name still pays its CP and source-cache scalar lookups; the
        // descriptor link pays one, then the first hierarchy edge pays one.
        let base = p.usage.work_units;
        p.options.max_work_units = base + 3;
        assert!(
            matches!(p.final_override(member),Err(TypeVerificationErrorKind::ResourceLimit{resource:TypeVerificationResource::WorkUnits,required,..}) if required==base+4)
        );
        assert_eq!(p.usage.work_units, base + 3);
    }
}
