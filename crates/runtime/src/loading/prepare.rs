use super::*;
use crate::ClassLayout;
use crabspot_classfile::{ClassFile, Member};
use std::collections::HashSet;
struct View<'a> {
    class: ClassId,
    metadata: &'a ClassFile,
    layout: &'a ClassLayout,
    loader: LoaderId,
    package: &'a [u16],
}
pub(super) struct Facts<'a> {
    views: Vec<View<'a>>,
    index: HashMap<ClassId, usize>,
    root: ClassId,
}
#[derive(Clone, Copy)]
struct Method<'a> {
    id: MethodId,
    name: &'a [u16],
    descriptor: &'a [u16],
    name_index: u16,
    descriptor_index: u16,
    flags: u16,
}
impl Method<'_> {
    fn instance(self) -> bool {
        self.flags & 8 == 0 && self.name.first() != Some(&60)
    }
    fn eligible(self) -> bool {
        self.instance() && self.flags & 2 == 0
    }
}
impl<'a> Facts<'a> {
    fn view(&self, class: ClassId, work: &mut Work) -> Result<&View<'a>> {
        work.step()?;
        self.index
            .get(&class)
            .and_then(|i| self.views.get(*i))
            .ok_or(E::InconsistentHierarchy { class: self.root })
    }
    fn method(&self, class: ClassId, index: usize, work: &mut Work) -> Result<Method<'a>> {
        work.step()?;
        let view = self.view(class, work)?;
        let member = view
            .metadata
            .methods
            .get(index)
            .ok_or(invalid("invalid preparation method index"))?;
        method(
            view.metadata,
            MethodId {
                owner: class,
                index,
            },
            member,
            work,
        )
    }
    fn superclass(&self, class: ClassId, work: &mut Work) -> Result<Option<ClassId>> {
        let node = self.view(class, work)?;
        let parent = node.layout.direct_superclass();
        if parent.is_some() {
            work.step()?;
        }
        Ok(parent)
    }
    fn interfaces(&self) -> impl Iterator<Item = &View<'a>> {
        self.views.iter()
    }
}
fn method<'a>(
    metadata: &'a ClassFile,
    id: MethodId,
    member: &Member,
    work: &mut Work,
) -> Result<Method<'a>> {
    work.step()?;
    let name = metadata
        .constant_pool
        .utf8(member.name_index)
        .map_err(RegistryError::Parse)?
        .units();
    work.step()?;
    let descriptor = metadata
        .constant_pool
        .utf8(member.descriptor_index)
        .map_err(RegistryError::Parse)?
        .units();
    Ok(Method {
        id,
        name,
        descriptor,
        name_index: member.name_index,
        descriptor_index: member.descriptor_index,
        flags: member.access_flags,
    })
}
fn matching(a: Method<'_>, b: Method<'_>, work: &mut Work) -> Result<bool> {
    if !(a.id.owner == b.id.owner && a.name_index == b.name_index) && !equal(work, a.name, b.name)?
    {
        return Ok(false);
    }
    if a.id.owner == b.id.owner && a.descriptor_index == b.descriptor_index {
        return Ok(true);
    }
    equal(work, a.descriptor, b.descriptor)
}
fn same_package(a: &View<'_>, b: &View<'_>, work: &mut Work) -> Result<bool> {
    if a.class == b.class {
        return Ok(true);
    }
    if a.loader != b.loader {
        return Ok(false);
    }
    equal(work, a.package, b.package)
}
pub(super) fn preflight<'a>(
    registry: &'a DefinitionRegistry,
    ledger: &Ledger,
    work: &mut Work,
    plan: &'a HierarchyPlan,
) -> Result<Facts<'a>> {
    work.step()?;
    registry.class(plan.root())?;
    let mut views = Vec::new();
    let mut index = HashMap::new();
    for class in plan.classes() {
        work.step()?;
        work.step()?;
        let metadata = registry.class(*class)?;
        work.step()?;
        let loader = registry.defining_loader(*class)?;
        work.step()?;
        let layout = plan
            .layout(*class)
            .ok_or(E::InconsistentHierarchy { class: plan.root() })?;
        if layout.class_id() != *class || index.contains_key(class) {
            return Err(E::InconsistentHierarchy { class: plan.root() });
        }
        work.charge(0, 2, 0)?;
        let name = metadata.name().map_err(RegistryError::Parse)?.units();
        work.utf(name.len())?;
        let end = name.iter().rposition(|u| *u == 47).unwrap_or(0);
        let package = &name[..end];
        work.scratch(2)?;
        reserve(&mut views, 1)?;
        index.try_reserve(1).map_err(|_| E::AllocationFailure)?;
        index.insert(*class, views.len());
        views.push(View {
            class: *class,
            metadata,
            layout,
            loader,
            package,
        });
    }
    let facts = Facts {
        views,
        index,
        root: plan.root(),
    };
    facts.view(plan.root(), work)?;
    for (row, view) in facts.views.iter().enumerate() {
        work.step()?;
        if (view.metadata.super_class == 0) != view.layout.direct_superclass().is_none()
            || view.metadata.interfaces.len() != view.layout.direct_interfaces().len()
        {
            return Err(E::InconsistentHierarchy { class: plan.root() });
        }
        if let Some(parent) = view.layout.direct_superclass() {
            edge(
                &facts,
                ledger,
                work,
                view,
                parent,
                view.metadata.super_class,
                row,
            )?;
        }
        for (cp, target) in view
            .metadata
            .interfaces
            .iter()
            .zip(view.layout.direct_interfaces())
        {
            edge(&facts, ledger, work, view, *target, *cp, row)?;
        }
    }
    Ok(facts)
}
fn edge(
    facts: &Facts<'_>,
    ledger: &Ledger,
    work: &mut Work,
    view: &View<'_>,
    target: ClassId,
    cp: u16,
    row: usize,
) -> Result<()> {
    work.step()?;
    work.step()?;
    let position = *facts
        .index
        .get(&target)
        .ok_or(E::InconsistentHierarchy { class: facts.root })?;
    if position >= row {
        return Err(E::InconsistentHierarchy { class: facts.root });
    }
    work.charge(0, 2, 0)?;
    let spelling = view
        .metadata
        .constant_pool
        .class_name(cp)
        .map_err(RegistryError::Parse)?
        .units();
    work.name(spelling)?;
    match ledger.lookup(spelling, view.loader, work)? {
        None => Err(E::MissingInitiation {
            origin: view.class,
            dependency: target,
        }),
        Some(actual) if actual != target => Err(E::InconsistentHierarchy { class: facts.root }),
        _ => Ok(()),
    }
}
fn eligible_target(
    root: &View<'_>,
    target: &View<'_>,
    flags: u16,
    work: &mut Work,
) -> Result<bool> {
    if flags & 2 != 0 {
        return Ok(false);
    }
    if flags & 5 != 0 {
        return Ok(true);
    }
    same_package(root, target, work)
}
fn append<T>(values: &mut Vec<T>, value: T, work: &mut Work) -> Result<()> {
    work.scratch(1)?;
    reserve(values, 1)?;
    values.push(value);
    Ok(())
}
fn package_reached(
    facts: &Facts<'_>,
    packages: &[ClassId],
    target: ClassId,
    work: &mut Work,
) -> Result<bool> {
    let target = facts.view(target, work)?;
    for class in packages {
        work.step()?;
        if same_package(facts.view(*class, work)?, target, work)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn emit(
    registry: &DefinitionRegistry,
    ledger: &mut Ledger,
    work: &mut Work,
    origin: Method<'_>,
    related: Method<'_>,
    left: LoaderId,
    right: LoaderId,
) -> Result<()> {
    let _ = registry;
    let site = work
        .site
        .as_mut()
        .ok_or(invalid("missing preparation site"))?;
    site.origin = Some(origin.id);
    site.related = Some(related.id);
    site.position = None;
    let descriptor = origin.descriptor;
    work.utf(descriptor.len())?;
    if descriptor.first() != Some(&40) {
        return Err(invalid("invalid preparation method descriptor"));
    }
    let mut p = 1;
    let mut parameter = 0;
    let mut slots = 1usize;
    while descriptor.get(p) != Some(&41) {
        if p >= descriptor.len() {
            return Err(invalid("missing method descriptor close"));
        }
        work.site
            .as_mut()
            .ok_or(invalid("missing preparation site"))?
            .position = Some(LoadingDescriptorPosition::Parameter(parameter));
        slots = slots
            .checked_add(leaf(ledger, work, descriptor, &mut p, false, left, right)?)
            .ok_or(invalid("preparation descriptor slot overflow"))?;
        if slots > 255 {
            return Err(invalid(
                "preparation descriptor exceeds instance slot limit",
            ));
        }
        parameter = parameter
            .checked_add(1)
            .ok_or(invalid("descriptor parameter overflow"))?;
    }
    p += 1;
    work.site
        .as_mut()
        .ok_or(invalid("missing preparation site"))?
        .position = Some(LoadingDescriptorPosition::Return);
    leaf(ledger, work, descriptor, &mut p, true, left, right)?;
    if p != descriptor.len() {
        return Err(invalid("trailing preparation descriptor units"));
    }
    let site = work
        .site
        .as_mut()
        .ok_or(invalid("missing preparation site"))?;
    site.related = None;
    site.position = None;
    Ok(())
}
fn leaf(
    ledger: &mut Ledger,
    work: &mut Work,
    descriptor: &[u16],
    p: &mut usize,
    void: bool,
    left: LoaderId,
    right: LoaderId,
) -> Result<usize> {
    let mut rank = 0;
    while descriptor.get(*p) == Some(&91) {
        rank += 1;
        if rank > 255 {
            return Err(invalid("preparation array rank exceeds255"));
        }
        *p += 1;
    }
    let tag = *descriptor
        .get(*p)
        .ok_or(invalid("missing preparation descriptor type"))?;
    *p += 1;
    match tag {
        66 | 67 | 68 | 70 | 73 | 74 | 83 | 90 => {}
        86 if void && rank == 0 => {}
        76 => {
            let start = *p;
            while descriptor.get(*p).is_some_and(|u| *u != 59) {
                *p += 1;
            }
            if descriptor.get(*p) != Some(&59) {
                return Err(invalid("unterminated nominal preparation type"));
            }
            let name = descriptor
                .get(start..*p)
                .ok_or(invalid("invalid nominal preparation span"))?;
            work.name(name).map_err(|error| match error {
                E::Registry(error)
                    if matches!(
                        error.as_ref(),
                        RegistryError::InvalidName(_) | RegistryError::UnsupportedDefinition(_)
                    ) =>
                {
                    invalid("invalid nominal preparation descriptor name")
                }
                other => other,
            })?;
            ledger.impose(name, left, right, work)?;
            *p += 1;
        }
        _ => return Err(invalid("invalid preparation descriptor tag")),
    }
    Ok(if rank != 0 {
        1
    } else {
        match tag {
            74 | 68 => 2,
            86 => 0,
            _ => 1,
        }
    })
}
pub(super) fn declared(
    registry: &DefinitionRegistry,
    facts: &Facts<'_>,
    ledger: &mut Ledger,
    work: &mut Work,
) -> Result<()> {
    let root = facts.view(facts.root, work)?;
    for index in 0..root.metadata.methods.len() {
        work.site = Some(LoadingPreparationSite {
            class: facts.root,
            origin: Some(MethodId {
                owner: facts.root,
                index,
            }),
            related: None,
            position: None,
        });
        let selected = facts.method(facts.root, index, work)?;
        if !selected.eligible() {
            continue;
        }
        let mut packages = Vec::new();
        append(&mut packages, facts.root, work)?;
        let mut current = facts.superclass(facts.root, work)?;
        for _ in 0..facts.views.len() {
            let Some(class) = current else { break };
            work.step()?;
            let target = facts.view(class, work)?;
            for index in 0..target.metadata.methods.len() {
                let candidate = facts.method(class, index, work)?;
                if !candidate.eligible() || !matching(selected, candidate, work)? {
                    continue;
                }
                if candidate.flags & 5 != 0 || package_reached(facts, &packages, class, work)? {
                    emit(
                        registry,
                        ledger,
                        work,
                        selected,
                        candidate,
                        root.loader,
                        target.loader,
                    )?;
                    if !package_reached(facts, &packages, class, work)? {
                        append(&mut packages, class, work)?;
                    }
                }
            }
            current = facts.superclass(class, work)?;
        }
        if current.is_some() {
            return Err(invalid("cyclic declared preparation ancestry"));
        }
        for target in facts.interfaces() {
            work.step()?;
            if target.class == facts.root || target.metadata.access_flags & 0x200 == 0 {
                continue;
            }
            for index in 0..target.metadata.methods.len() {
                let candidate = facts.method(target.class, index, work)?;
                if candidate.eligible()
                    && matching(selected, candidate, work)?
                    && eligible_target(root, target, candidate.flags, work)?
                {
                    emit(
                        registry,
                        ledger,
                        work,
                        selected,
                        candidate,
                        root.loader,
                        target.loader,
                    )?;
                }
            }
        }
    }
    Ok(())
}
fn strict_subinterface(
    facts: &Facts<'_>,
    descendant: ClassId,
    ancestor: ClassId,
    work: &mut Work,
) -> Result<bool> {
    let mut pending = Vec::new();
    let mut seen = HashSet::new();
    enqueue(facts, descendant, &mut pending, work)?;
    while let Some(class) = pending.pop() {
        work.step()?;
        if !seen.contains(&class) {
            work.scratch(1)?;
            seen.try_reserve(1).map_err(|_| E::AllocationFailure)?;
            seen.insert(class);
            if class == ancestor {
                return Ok(true);
            }
            enqueue(facts, class, &mut pending, work)?;
        }
    }
    Ok(false)
}
fn enqueue(
    facts: &Facts<'_>,
    class: ClassId,
    pending: &mut Vec<ClassId>,
    work: &mut Work,
) -> Result<()> {
    let node = facts.view(class, work)?;
    for target in node.layout.direct_interfaces().iter().rev() {
        work.step()?;
        append(pending, *target, work)?;
    }
    Ok(())
}
fn choose<'a>(
    facts: &Facts<'a>,
    target: Method<'a>,
    work: &mut Work,
) -> Result<Option<Method<'a>>> {
    if target.flags & 2 != 0 {
        return Ok(Some(target));
    }
    let owner = facts.view(target.id.owner, work)?;
    let mut current = Some(facts.root);
    for _ in 0..facts.views.len() {
        let Some(class) = current else { break };
        work.step()?;
        let view = facts.view(class, work)?;
        for index in 0..view.metadata.methods.len() {
            let candidate = facts.method(class, index, work)?;
            if candidate.eligible()
                && matching(candidate, target, work)?
                && eligible_target(view, owner, target.flags, work)?
            {
                return Ok(Some(candidate));
            }
        }
        current = facts.superclass(class, work)?;
    }
    if current.is_some() {
        return Err(invalid("cyclic selected preparation ancestry"));
    }
    let mut candidates = Vec::new();
    for view in facts.interfaces() {
        work.step()?;
        if view.class == facts.root || view.metadata.access_flags & 0x200 == 0 {
            continue;
        }
        for index in 0..view.metadata.methods.len() {
            let candidate = facts.method(view.class, index, work)?;
            if candidate.eligible() && matching(candidate, target, work)? {
                append(&mut candidates, candidate, work)?;
            }
        }
    }
    let mut selected = None;
    for (i, candidate) in candidates.iter().enumerate() {
        let mut dominated = false;
        for (j, other) in candidates.iter().enumerate() {
            work.step()?;
            if i != j && strict_subinterface(facts, other.id.owner, candidate.id.owner, work)? {
                dominated = true;
                break;
            }
        }
        if !dominated && candidate.flags & 0x400 == 0 {
            if selected.is_some() {
                return Ok(None);
            }
            selected = Some(*candidate)
        }
    }
    Ok(selected)
}
pub(super) fn inherited(
    registry: &DefinitionRegistry,
    facts: &Facts<'_>,
    ledger: &mut Ledger,
    work: &mut Work,
) -> Result<()> {
    work.site = Some(LoadingPreparationSite {
        class: facts.root,
        origin: None,
        related: None,
        position: None,
    });
    let root = facts.view(facts.root, work)?;
    for view in facts.interfaces() {
        work.site = Some(LoadingPreparationSite {
            class: facts.root,
            origin: None,
            related: None,
            position: None,
        });
        work.step()?;
        if view.class == facts.root || view.metadata.access_flags & 0x200 == 0 {
            continue;
        }
        for index in 0..view.metadata.methods.len() {
            let id = MethodId {
                owner: view.class,
                index,
            };
            work.site = Some(LoadingPreparationSite {
                class: facts.root,
                origin: Some(id),
                related: None,
                position: None,
            });
            let target = facts.method(view.class, index, work)?;
            if !target.instance() {
                continue;
            }
            let mut own = false;
            for index in 0..root.metadata.methods.len() {
                let candidate = facts.method(facts.root, index, work)?;
                if candidate.eligible()
                    && matching(candidate, target, work)?
                    && eligible_target(root, view, target.flags, work)?
                {
                    own = true;
                    break;
                }
            }
            if own {
                continue;
            }
            if let Some(selected) = choose(facts, target, work)? {
                work.site = Some(LoadingPreparationSite {
                    class: facts.root,
                    origin: Some(target.id),
                    related: Some(selected.id),
                    position: None,
                });
                let selected_view = facts.view(selected.id.owner, work)?;
                emit(
                    registry,
                    ledger,
                    work,
                    target,
                    selected,
                    selected_view.loader,
                    view.loader,
                )?;
            }
        }
    }
    Ok(())
}
pub(super) fn prepare(
    registry: &DefinitionRegistry,
    ledger: &mut Ledger,
    work: &mut Work,
    plan: &HierarchyPlan,
) -> Result<()> {
    let facts = preflight(registry, ledger, work, plan)?;
    declared(registry, &facts, ledger, work)?;
    inherited(registry, &facts, ledger, work)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{definition, snapshot};
    use super::*;
    #[test]
    fn malformed_later_descriptor_preserves_prior_obligation_and_precise_site() {
        let mut d =
            LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        let object = d
            .context(LoadingWorkLimits::default())
            .define(
                a,
                &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                &definition("java/lang/Object", None, &[], 0x21, &[]),
                ParseOptions::default(),
            )
            .unwrap();
        let descriptor: Vec<_> = "(Lp/P;Lbad.name;)V".encode_utf16().collect();
        let name: Vec<_> = "m".encode_utf16().collect();
        let id = MethodId {
            owner: object,
            index: 0,
        };
        let method = Method {
            id,
            name: &name,
            descriptor: &descriptor,
            name_index: 0,
            descriptor_index: 0,
            flags: 1,
        };
        let mut work = Work {
            limits: LoadingWorkLimits::default(),
            usage: LoadingWorkUsage::default(),
            site: Some(LoadingPreparationSite {
                class: object,
                origin: Some(id),
                related: None,
                position: None,
            }),
            fail_reserve_after: None,
        };
        assert!(matches!(
            emit(&d.registry, &mut d.ledger, &mut work, method, method, a, b),
            Err(E::InvalidMetadata(_))
        ));
        assert_eq!(
            work.site.unwrap().position,
            Some(LoadingDescriptorPosition::Parameter(1))
        );
        let p: Vec<_> = "p/P".encode_utf16().collect();
        work.name(&p).unwrap();
        assert!(d.ledger.constrained(&p, a, b, &mut work).unwrap());
    }
    #[test]
    fn malformed_slot_rank_void_and_trailing_shapes_cannot_emit_success() {
        let mut d =
            LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap();
        let a = d.create_loader().unwrap();
        let object = d
            .context(LoadingWorkLimits::default())
            .define(
                a,
                &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                &definition("java/lang/Object", None, &[], 0x21, &[]),
                ParseOptions::default(),
            )
            .unwrap();
        let bad = [
            "([V)V".to_string(),
            "()Vx".to_string(),
            format!("({})V", "I".repeat(255)),
            format!("({}I)V", "[".repeat(256)),
            "(L;)V".to_string(),
        ];
        let n: Vec<_> = "m".encode_utf16().collect();
        for descriptor in bad {
            let descriptor: Vec<_> = descriptor.encode_utf16().collect();
            let method = Method {
                id: MethodId {
                    owner: object,
                    index: 0,
                },
                name: &n,
                descriptor: &descriptor,
                name_index: 0,
                descriptor_index: 0,
                flags: 1,
            };
            let mut work = Work {
                limits: LoadingWorkLimits::default(),
                usage: LoadingWorkUsage::default(),
                site: Some(LoadingPreparationSite {
                    class: object,
                    origin: None,
                    related: None,
                    position: None,
                }),
                fail_reserve_after: None,
            };
            let before = snapshot(&d.ledger);
            assert!(matches!(
                emit(&d.registry, &mut d.ledger, &mut work, method, method, a, a),
                Err(E::InvalidMetadata(_))
            ));
            assert_eq!(snapshot(&d.ledger), before);
        }
    }
}
