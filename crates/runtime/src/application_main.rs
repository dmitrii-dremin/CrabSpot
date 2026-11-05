//! Opaque checked application-main selection. This is not a verification proof.
use crate::{ClassId, FieldAccessError, FieldResolutionContext, MethodId, RegistryError};
use std::fmt;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationMainError {
    Context(FieldAccessError),
    NoEligibleMain { named: ClassId },
    WrongMainFlags { method: MethodId },
    TokenMismatch,
}
impl fmt::Display for ApplicationMainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(e) => write!(f, "application main context failed: {e}"),
            Self::NoEligibleMain { named } => {
                write!(f, "no public main([Ljava/lang/String;)V for {named:?}")
            }
            Self::WrongMainFlags { method } => write!(
                f,
                "application main {method:?} must be ordinary public static Code"
            ),
            Self::TokenMismatch => {
                f.write_str("application main token belongs to another checked snapshot")
            }
        }
    }
}
impl std::error::Error for ApplicationMainError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(e) => Some(e),
            _ => None,
        }
    }
}
fn context(e: FieldAccessError) -> Box<ApplicationMainError> {
    Box::new(ApplicationMainError::Context(e))
}
#[derive(Debug)]
pub struct ResolvedApplicationMain {
    registry: u64,
    heap: u64,
    layouts: usize,
    named: ClassId,
    method: MethodId,
}
impl ResolvedApplicationMain {
    pub fn named_class(&self) -> ClassId {
        self.named
    }
    pub fn method(&self) -> MethodId {
        self.method
    }
}
impl FieldResolutionContext<'_> {
    /// Select only a checked public static entry; no CP site, loading equality,
    /// Java allocation, initialization or execution is performed.
    pub fn resolve_application_main(
        &mut self,
        named: ClassId,
    ) -> Result<ResolvedApplicationMain, Box<ApplicationMainError>> {
        self.resolve_application_main_mode(named, false)
    }
    /// Select the same checked entry while also permitting synchronized Code.
    /// The returned token does not enable monitoring in a legacy session.
    pub fn resolve_application_main_with_monitors(
        &mut self,
        named: ClassId,
    ) -> Result<ResolvedApplicationMain, Box<ApplicationMainError>> {
        self.resolve_application_main_mode(named, true)
    }
    fn resolve_application_main_mode(
        &mut self,
        named: ClassId,
        monitors: bool,
    ) -> Result<ResolvedApplicationMain, Box<ApplicationMainError>> {
        self.unchanged().map_err(context)?;
        if self.member_loading_work().is_none() {
            return Err(context(FieldAccessError::InvalidState(
                "application main requires a checked loading domain",
            )));
        }
        self.registry()
            .class(named)
            .map_err(|e| context(e.into()))?;
        self.heap()
            .instance_graph_node(named)
            .map_err(|e| context(e.into()))?;
        let mut current = Some(named);
        for _ in 0..self.heap().retained_layout_classes() {
            let Some(class) = current else { break };
            self.step().map_err(context)?;
            let registry = self.registry();
            let metadata = registry.class(class).map_err(|e| context(e.into()))?;
            self.heap()
                .instance_graph_node(class)
                .map_err(|e| context(e.into()))?;
            for (index, member) in metadata.methods.iter().enumerate() {
                self.step().map_err(context)?;
                let name = metadata
                    .constant_pool
                    .utf8(member.name_index)
                    .map_err(|e| context(RegistryError::Parse(e).into()))?;
                self.charge_bulk_work(name.units().len(), 0)
                    .map_err(context)?;
                if !name.equals_ascii("main") {
                    continue;
                }
                let descriptor = metadata
                    .constant_pool
                    .utf8(member.descriptor_index)
                    .map_err(|e| context(RegistryError::Parse(e).into()))?;
                self.charge_bulk_work(descriptor.units().len(), 0)
                    .map_err(context)?;
                if !descriptor.equals_ascii("([Ljava/lang/String;)V")
                    || member.access_flags & 1 == 0
                {
                    continue;
                }
                let method = MethodId {
                    owner: class,
                    index,
                };
                if member.access_flags & 8 == 0
                    || member.access_flags & (0x100 | 0x400) != 0
                    || (!monitors && member.access_flags & 0x20 != 0)
                {
                    return Err(Box::new(ApplicationMainError::WrongMainFlags { method }));
                }
                return Ok(ResolvedApplicationMain {
                    registry: registry.identity(),
                    heap: self.heap().identity,
                    layouts: self.heap().retained_layout_classes(),
                    named,
                    method,
                });
            }
            current = if metadata.access_flags & 0x200 != 0 {
                None
            } else {
                self.heap()
                    .instance_graph_node(class)
                    .map_err(|e| context(e.into()))?
                    .superclass
            };
        }
        if current.is_some() {
            return Err(context(FieldAccessError::InvalidState(
                "application main superclass chain is cyclic",
            )));
        }
        Err(Box::new(ApplicationMainError::NoEligibleMain { named }))
    }
    /// Recheck the opaque token without resolving or allocating another entry.
    pub fn check_application_main(
        &self,
        entry: &ResolvedApplicationMain,
    ) -> Result<(), Box<ApplicationMainError>> {
        self.unchanged().map_err(context)?;
        if self.member_loading_work().is_none() {
            return Err(context(FieldAccessError::InvalidState(
                "application main requires a checked loading domain",
            )));
        }
        if entry.registry != self.registry().identity()
            || entry.heap != self.heap().identity
            || entry.layouts != self.heap().retained_layout_classes()
        {
            return Err(Box::new(ApplicationMainError::TokenMismatch));
        }
        self.registry()
            .class(entry.named)
            .map_err(|e| context(e.into()))?;
        self.registry()
            .method(entry.method)
            .map_err(|e| context(e.into()))?;
        self.heap()
            .instance_graph_node(entry.named)
            .map_err(|e| context(e.into()))?;
        self.heap()
            .instance_graph_node(entry.method.owner())
            .map_err(|e| context(e.into()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{LoadingDomain, ManagedHeap, SameLoaderResolver, build_hierarchy};
    fn bytes(name: &str, root: bool) -> Vec<u8> {
        bytes_flags(name, root, 9)
    }
    fn bytes_flags(name: &str, root: bool, flags: u16) -> Vec<u8> {
        fn w(out: &mut Vec<u8>, n: u16) {
            out.extend(n.to_be_bytes());
        }
        let mut out = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 8];
        for text in [
            name,
            "java/lang/Object",
            "main",
            "([Ljava/lang/String;)V",
            "Code",
        ]
        .into_iter()
        .enumerate()
        {
            let (index, text) = text;
            out.push(1);
            w(&mut out, text.len() as u16);
            out.extend(text.bytes());
            if index < 2 {
                out.push(7);
                w(&mut out, if index == 0 { 1 } else { 3 });
            }
        }
        for n in [0x21, 2, if root { 0 } else { 4 }, 0, 0, 1, flags, 5, 6] {
            w(&mut out, n);
        }
        if flags & 0x100 != 0 {
            w(&mut out, 0);
            w(&mut out, 0);
            return out;
        }
        w(&mut out, 1);
        w(&mut out, 7);
        out.extend(13_u32.to_be_bytes());
        w(&mut out, 0);
        w(&mut out, 1);
        out.extend(1_u32.to_be_bytes());
        out.push(0xb1);
        w(&mut out, 0);
        w(&mut out, 0);
        w(&mut out, 0);
        out
    }
    #[test]
    fn private_fingerprint_and_handle_damage_rejects_without_charged_scan_or_publication() {
        let mut domain = LoadingDomain::new(Default::default(), Default::default()).unwrap();
        let boot = domain.create_loader().unwrap();
        let object = domain
            .context(Default::default())
            .define(
                boot,
                &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                &bytes("java/lang/Object", true),
                Default::default(),
            )
            .unwrap();
        let named = domain
            .context(Default::default())
            .define(boot, &[68], &bytes("D", false), Default::default())
            .unwrap();
        domain.enable_member_resolution(Default::default()).unwrap();
        let plans = [object, named].map(|c| {
            build_hierarchy(
                domain.registry(),
                c,
                &mut SameLoaderResolver,
                Default::default(),
            )
            .unwrap()
        });
        for p in &plans {
            domain
                .context(Default::default())
                .prepare_member_owner(p)
                .unwrap();
        }
        let mut heap =
            ManagedHeap::new_with_instances(Default::default(), Default::default()).unwrap();
        heap.admit_instance_layouts(domain.registry(), &plans.iter().collect::<Vec<_>>())
            .unwrap();
        let mut context = domain
            .member_context_unnamed_modules(&mut heap, Default::default(), Default::default())
            .unwrap();
        for which in 0..5 {
            let mut token = context.resolve_application_main(named).unwrap();
            match which {
                0 => token.registry = 0,
                1 => token.heap = 0,
                2 => token.layouts += 1,
                3 => token.method.index = usize::MAX,
                _ => token.named.index = usize::MAX,
            };
            let before = context.lookup_steps();
            let loading = context.member_loading_work();
            assert!(context.check_application_main(&token).is_err());
            assert_eq!(context.lookup_steps(), before);
            assert_eq!(context.member_loading_work(), loading);
            assert_eq!(context.heap().allocated_instances(), 0);
            assert_eq!(context.heap().allocated_arrays(), 0);
        }
        drop(context);
        assert_eq!(domain.member_resolution_usage().sites, 0);
    }
    #[test]
    fn monitor_selector_only_relaxes_sync_with_identical_charged_lookup() {
        for flags in [9, 0x29, 0x129] {
            let mut domain = LoadingDomain::new(Default::default(), Default::default()).unwrap();
            let loader = domain.create_loader().unwrap();
            let object = domain
                .context(Default::default())
                .define(
                    loader,
                    &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                    &bytes("java/lang/Object", true),
                    Default::default(),
                )
                .unwrap();
            let named = domain
                .context(Default::default())
                .define(
                    loader,
                    &[68],
                    &bytes_flags("D", false, flags),
                    Default::default(),
                )
                .unwrap();
            domain.enable_member_resolution(Default::default()).unwrap();
            let plans = [object, named].map(|c| {
                build_hierarchy(
                    domain.registry(),
                    c,
                    &mut SameLoaderResolver,
                    Default::default(),
                )
                .unwrap()
            });
            for p in &plans {
                domain
                    .context(Default::default())
                    .prepare_member_owner(p)
                    .unwrap()
            }
            let mut heap =
                ManagedHeap::new_with_instances(Default::default(), Default::default()).unwrap();
            heap.admit_instance_layouts(domain.registry(), &plans.iter().collect::<Vec<_>>())
                .unwrap();
            let mut context = domain
                .member_context_unnamed_modules(&mut heap, Default::default(), Default::default())
                .unwrap();
            let before = context.lookup_steps();
            let legacy = context.resolve_application_main(named);
            let legacy_work = context.lookup_steps() - before;
            let before = context.lookup_steps();
            let monitored = context.resolve_application_main_with_monitors(named);
            let monitor_work = context.lookup_steps() - before;
            assert_eq!(legacy_work, monitor_work);
            if flags == 9 {
                assert!(legacy.is_ok());
                assert!(monitored.is_ok())
            } else if flags == 0x29 {
                assert!(matches!(
                    *legacy.unwrap_err(),
                    super::ApplicationMainError::WrongMainFlags { .. }
                ));
                let token = monitored.unwrap();
                assert_eq!(token.named_class(), named);
                assert_eq!(token.method().owner(), named);
                context.check_application_main(&token).unwrap();
            } else {
                assert!(legacy.is_err());
                assert!(matches!(
                    *monitored.unwrap_err(),
                    super::ApplicationMainError::WrongMainFlags { .. }
                ));
            }
            assert_eq!(context.heap().allocated_instances(), 0);
            assert_eq!(context.heap().allocated_arrays(), 0);
        }
    }
}
