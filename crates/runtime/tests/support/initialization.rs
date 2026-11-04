// Independent synthetic class-file fixtures shared by initialization public and
// private guards; no reference JVM or production metadata builder is used.
use super::rt::*;
use crabspot_classfile::ParseOptions;
pub fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}
#[derive(Clone, Copy)]
pub enum Cv<'a> {
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    String(&'a str),
}
pub struct Field<'a> {
    pub name: &'a str,
    pub desc: &'a str,
    pub flags: u16,
    pub value: Option<Cv<'a>>,
}
impl<'a> Field<'a> {
    pub fn new(name: &'a str, desc: &'a str, flags: u16, value: Option<Cv<'a>>) -> Self {
        Self {
            name,
            desc,
            flags,
            value,
        }
    }
}
pub struct Method<'a> {
    pub name: &'a str,
    pub desc: &'a str,
    pub flags: u16,
    pub code: bool,
}
impl<'a> Method<'a> {
    pub fn new(name: &'a str, desc: &'a str, flags: u16) -> Self {
        Self {
            name,
            desc,
            flags,
            code: flags & (0x100 | 0x400) == 0 || name == "<clinit>",
        }
    }
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let index = (self.0.len() + 1) as u16;
        let mut bytes = vec![tag];
        bytes.extend(payload);
        self.0.push(bytes);
        if matches!(tag, 5 | 6) {
            self.0.push(Vec::new());
        }
        index
    }
    fn text(&mut self, text: &str) -> u16 {
        let mut encoded = Vec::new();
        for unit in text.encode_utf16() {
            match unit {
                1..=127 => encoded.push(unit as u8),
                0..=0x7ff => encoded.extend([0xc0 | (unit >> 6) as u8, 0x80 | (unit & 63) as u8]),
                _ => encoded.extend([
                    0xe0 | (unit >> 12) as u8,
                    0x80 | ((unit >> 6) & 63) as u8,
                    0x80 | (unit & 63) as u8,
                ]),
            }
        }
        let mut data = words(&[u16::try_from(encoded.len()).unwrap()]);
        data.extend(encoded);
        if let Some(index) = self
            .0
            .iter()
            .position(|entry| entry.first() == Some(&1) && entry.get(1..) == Some(data.as_slice()))
        {
            return (index + 1) as u16;
        }
        self.add(1, &data)
    }
    fn class(&mut self, name: &str) -> u16 {
        let name = self.text(name);
        self.add(7, &words(&[name]))
    }
    fn value(&mut self, value: Cv<'_>) -> u16 {
        match value {
            Cv::Int(value) => self.add(3, &value.to_be_bytes()),
            Cv::Long(value) => self.add(5, &value.to_be_bytes()),
            Cv::Float(value) => self.add(4, &value.to_be_bytes()),
            Cv::Double(value) => self.add(6, &value.to_be_bytes()),
            Cv::String(text) => {
                let text = self.text(text);
                self.add(8, &words(&[text]))
            }
        }
    }
}
pub struct Def {
    pub class: ClassId,
    pub refs: Vec<u16>,
    pub methods: Vec<MethodId>,
}
#[allow(clippy::too_many_arguments)]
pub fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[Field<'_>],
    methods: &[Method<'_>],
    refs: &[(&str, &str, &str)],
    major: u16,
) -> Def {
    let mut pool = Pool::default();
    let own = pool.class(name);
    let parent = parent.map_or(0, |name| pool.class(name));
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let cv_name = pool.text("ConstantValue");
    let code_name = pool.text("Code");
    let fields: Vec<_> = fields
        .iter()
        .map(|field| {
            (
                pool.text(field.name),
                pool.text(field.desc),
                field.flags,
                field.value.map(|value| pool.value(value)),
            )
        })
        .collect();
    let method_data: Vec<_> = methods
        .iter()
        .map(|method| {
            (
                pool.text(method.name),
                pool.text(method.desc),
                method.flags,
                method.code,
            )
        })
        .collect();
    let refs: Vec<_> = refs
        .iter()
        .map(|(owner, name, desc)| {
            let owner = pool.class(owner);
            let name = pool.text(name);
            let desc = pool.text(desc);
            let nat = pool.add(12, &words(&[name, desc]));
            pool.add(9, &words(&[owner, nat]))
        })
        .collect();
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0];
    bytes.extend(major.to_be_bytes());
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, own, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[fields.len() as u16]));
    for (name, desc, flags, cv) in fields {
        bytes.extend(words(&[flags, name, desc, u16::from(cv.is_some())]));
        if let Some(cv) = cv {
            bytes.extend(words(&[cv_name]));
            bytes.extend(2_u32.to_be_bytes());
            bytes.extend(words(&[cv]));
        }
    }
    bytes.extend(words(&[method_data.len() as u16]));
    for (name, desc, flags, code) in method_data {
        bytes.extend(words(&[flags, name, desc, u16::from(code)]));
        if code {
            bytes.extend(words(&[code_name]));
            bytes.extend(13_u32.to_be_bytes());
            bytes.extend(words(&[1, 4]));
            bytes.extend(1_u32.to_be_bytes());
            bytes.extend([0xb1]);
            bytes.extend(words(&[0, 0]));
        }
    }
    bytes.extend(words(&[0]));
    let class = registry
        .define(loader, &units(name), &bytes, ParseOptions::default())
        .unwrap();
    let methods = (0..methods.len())
        .map(|index| {
            registry
                .find_declared_method(
                    class,
                    &units(methods[index].name),
                    &units(methods[index].desc),
                )
                .unwrap()
                .unwrap()
        })
        .collect();
    Def {
        class,
        refs,
        methods,
    }
}
pub struct World {
    pub registry: DefinitionRegistry,
    pub u: ReferenceTypeUniverse,
    pub throwable: ClassId,
    pub error: ClassId,
}
pub fn world() -> World {
    world_with_error("java/lang/Error", Some("java/lang/Throwable"), 0x21)
}
pub fn world_with_error(error_name: &str, error_parent: Option<&str>, error_flags: u16) -> World {
    world_options(
        error_name,
        error_parent,
        error_flags,
        RegistryOptions::default(),
    )
}
pub fn world_options(
    error_name: &str,
    error_parent: Option<&str>,
    error_flags: u16,
    options: RegistryOptions,
) -> World {
    let mut registry = DefinitionRegistry::new(options).unwrap();
    let l = registry.create_loader().unwrap();
    let mut simple = |name, parent, flags| {
        define(
            &mut registry,
            l,
            name,
            parent,
            &[],
            flags,
            &[],
            &[],
            &[],
            65,
        )
        .class
    };
    let object = simple("java/lang/Object", None, 0x21);
    let cloneable = simple("java/lang/Cloneable", Some("java/lang/Object"), 0x601);
    let serializable = simple("java/io/Serializable", Some("java/lang/Object"), 0x601);
    let throwable = simple("java/lang/Throwable", Some("java/lang/Object"), 0x21);
    let error = simple(error_name, error_parent, error_flags);
    World {
        registry,
        u: ReferenceTypeUniverse {
            bootstrap_loader: l,
            object,
            cloneable,
            serializable,
        },
        throwable,
        error,
    }
}
pub fn heap(w: &World, roots: &[ClassId], resolver: &mut impl HierarchyResolver) -> ManagedHeap {
    let plans: Vec<_> = roots
        .iter()
        .chain([&w.u.cloneable, &w.u.serializable, &w.throwable])
        .map(|class| {
            build_hierarchy(&w.registry, *class, resolver, HierarchyOptions::default()).unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&w.registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&w.registry, w.u).unwrap();
    heap
}
pub fn context<'a>(
    w: &'a World,
    heap: &'a mut ManagedHeap,
    resolver: &'a mut impl HierarchyResolver,
) -> FieldResolutionContext<'a> {
    FieldResolutionContext::new_unnamed_modules(
        &w.registry,
        heap,
        resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap()
}
pub fn profiles(
    ctx: &mut FieldResolutionContext<'_>,
    throwable: ClassId,
) -> (StaticStateProfile, ExceptionProfile) {
    let statics = ctx
        .allocate_static_state(StaticStateOptions::default())
        .unwrap();
    let exceptions = ctx.prepare_exception_profile(throwable).unwrap();
    (statics, exceptions)
}
pub fn field(w: &World, class: ClassId, name: &str, desc: &str) -> FieldId {
    w.registry
        .find_declared_field(class, &units(name), &units(desc))
        .unwrap()
        .unwrap()
}
pub fn limits() -> InitializationPlanLimits {
    InitializationPlanLimits {
        max_dependencies: usize::MAX,
        max_constants: usize::MAX,
    }
}
