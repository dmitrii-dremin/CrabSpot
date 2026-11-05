//! Replay preparation and subsequent loads against preserved fresh-loader JDK 21 observations.
//! HotSpot's trigger is class initialization. The runtime operation being compared is preparation.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HierarchyOptions, HierarchyResolver, LoaderId,
    LoadingConstraintError, LoadingConstraintErrorKind, LoadingDomain, ResolveError,
    build_hierarchy,
};
use std::{collections::HashMap, fs, path::Path};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/");
const ROWS: &str = include_str!("../../../tests/fixtures/loading-constraints/results.tsv");
const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
const NAMES: [&str; 3] = ["A", "B", "C"];

fn units(name: &str) -> Vec<u16> {
    name.encode_utf16().collect()
}

#[derive(Default)]
struct Routes(HashMap<(LoaderId, Vec<u16>), ClassId>);

impl HierarchyResolver for Routes {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        if let Some(id) = self.0.get(&(loader, name.to_vec())) {
            return Ok(Some(*id));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|e| ResolveError::new(e.to_string()))
    }
}

struct World {
    inputs: &'static str,
    domain: LoadingDomain,
    loaders: [LoaderId; 3],
    hierarchy: Routes,
    value_routes: HashMap<(usize, String), usize>,
}

impl World {
    fn new(inputs: &'static str) -> Self {
        let mut domain = LoadingDomain::new(Default::default(), Default::default()).unwrap();
        let boot = domain.create_loader().unwrap();
        let loaders = std::array::from_fn(|_| domain.create_loader().unwrap());
        let mut world = Self {
            inputs,
            domain,
            loaders,
            hierarchy: Routes::default(),
            value_routes: HashMap::new(),
        };
        let object = "java/lang/Object";
        let bytes = include_bytes!(
            "../../../tests/fixtures/type-state-facts/classes/java/lang/Object.class"
        );
        let id = world
            .domain
            .context(Default::default())
            .define(boot, &units(object), bytes, ParseOptions::default())
            .unwrap();
        for owner in [A, B, C] {
            world.alias(owner, object, id);
        }
        world
    }

    fn define(&mut self, owner: usize, name: &str) -> Result<ClassId, Box<LoadingConstraintError>> {
        let path = Path::new(FIXTURES)
            .join("loading-constraints")
            .join(self.inputs)
            .join(format!("{name}.class"));
        let data = fs::read(path).unwrap();
        self.domain.context(Default::default()).define(
            self.loaders[owner],
            &units(name),
            &data,
            ParseOptions::default(),
        )
    }

    fn alias(&mut self, client: usize, name: &str, id: ClassId) {
        let loader = self.loaders[client];
        let key = units(name);
        self.domain
            .context(Default::default())
            .record_initiation(loader, &key, id)
            .unwrap();
        self.hierarchy.0.insert((loader, key), id);
    }

    fn parent(&mut self, owner: usize, client: usize, name: &str) {
        let id = self.define(owner, name).unwrap();
        self.alias(client, name, id);
    }

    fn prepare(&mut self, owner: usize, name: &str) -> &'static str {
        let root = self.define(owner, name).unwrap();
        let plan = build_hierarchy(
            self.domain.registry(),
            root,
            &mut self.hierarchy,
            HierarchyOptions::default(),
        )
        .unwrap();
        match self
            .domain
            .context(Default::default())
            .prepare_constraints(&plan)
        {
            Ok(()) => "ok",
            Err(e)
                if matches!(
                    e.kind,
                    LoadingConstraintErrorKind::ConstraintConflict { .. }
                ) =>
            {
                "LinkageError"
            }
            Err(e) => panic!("unexpected preparation outcome for {name}: {e}"),
        }
    }

    fn delegate_value(&mut self, from: usize, leaf: &str, to: usize) {
        self.value_routes.insert((from, leaf.to_owned()), to);
    }

    fn load_value(
        &mut self,
        owner: usize,
        leaf: &str,
    ) -> Result<ClassId, Box<LoadingConstraintError>> {
        let name = format!("value/{leaf}");
        let loader = self.loaders[owner];
        let key = units(&name);
        if let Some(id) = self
            .domain
            .context(Default::default())
            .lookup_initiated(loader, &key)?
        {
            return Ok(id);
        }
        if let Some(to) = self.value_routes.get(&(owner, leaf.to_owned())).copied() {
            // Each fresh fixture has either a single directed route or a pair of local definitions.
            assert_ne!(owner, to);
            assert!(!self.value_routes.contains_key(&(to, leaf.to_owned())));
            let id = self.load_value(to, leaf)?;
            self.domain
                .context(Default::default())
                .record_initiation(loader, &key, id)?;
            return Ok(id);
        }
        self.define(owner, &name)
    }

    fn value(&mut self, owner: usize, leaf: &str) -> &'static str {
        match self.load_value(owner, leaf) {
            Ok(id) => {
                let actual = self.domain.registry().defining_loader(id).unwrap();
                NAMES[self
                    .loaders
                    .iter()
                    .position(|loader| *loader == actual)
                    .unwrap()]
            }
            Err(e)
                if matches!(
                    e.kind,
                    LoadingConstraintErrorKind::DefinitionConflict { .. }
                        | LoadingConstraintErrorKind::InitiationConflict { .. }
                ) =>
            {
                "LinkageError"
            }
            Err(e) => panic!("unexpected load outcome for {leaf}: {e}"),
        }
    }
}

fn timing(w: &mut World, r: &[&str]) {
    let [
        family,
        order,
        before_l,
        before_r,
        stage,
        outcome,
        after_l,
        after_r,
        repair,
    ] = *r
    else {
        panic!("invalid timing row")
    };
    let (root, left, right) = match family {
        "Inherit" => {
            w.parent(A, B, "parent/Impl");
            w.parent(C, B, "iface/Service");
            ("child/Inherit".to_owned(), A, C)
        }
        "Declare" => {
            w.parent(C, B, "iface/Service");
            ("child/Declare".to_owned(), C, B)
        }
        _ => {
            w.parent(A, B, &format!("parent/Base{family}"));
            (format!("child/Sub{family}"), A, B)
        }
    };
    if order == "same-before" {
        w.delegate_value(right, "K", left);
    }
    if before_l != "-" {
        assert_eq!(w.value(left, "K"), before_l);
    }
    if before_r != "-" {
        assert_eq!(w.value(right, "K"), before_r);
    }
    assert_eq!(stage, "initialize");
    assert_eq!(w.prepare(B, &root), outcome);
    if outcome == "ok" {
        assert_eq!(w.value(left, "K"), after_l);
        assert_eq!(w.value(right, "K"), after_r);
        if repair != "-" {
            let (failed, target) = if after_r == "LinkageError" {
                (right, left)
            } else {
                (left, right)
            };
            w.delegate_value(failed, "K", target);
            assert_eq!(w.value(failed, "K"), repair);
        }
    }
}

fn selection(w: &mut World, r: &[&str]) {
    let [root, order, stage, outcome, after] = *r else {
        panic!("invalid selection row")
    };
    let kinds = root.strip_prefix("child.").unwrap();
    let left = ["Abstract", "Default", "Private"]
        .into_iter()
        .find(|v| kinds.starts_with(v))
        .unwrap();
    let right = &kinds[left.len()..];
    w.parent(A, B, &format!("iface/Left{left}"));
    w.parent(C, B, &format!("iface/Right{right}"));
    if order == "same" {
        w.delegate_value(C, "K", A);
    }
    if order != "after" {
        assert_eq!(w.value(A, "K"), "A");
        assert_eq!(w.value(C, "K"), if order == "same" { "A" } else { "C" });
    }
    let prep = w.prepare(B, &root.replace('.', "/"));
    match stage {
        "initialize" => assert_eq!(prep, outcome),
        "invoke" => {
            // The Java driver invokes a method only after preparation succeeded.
            assert_eq!(prep, "ok");
            assert_eq!(format!("{}/{}", w.value(A, "K"), w.value(C, "K")), after);
        }
        _ => panic!("unexpected stage {stage}"),
    }
}

fn transitive(w: &mut World, r: &[&str]) {
    match *r {
        [
            "transitive",
            reverse,
            prime,
            first,
            second,
            values,
            "initialize",
            "initialize",
        ] => {
            w.parent(A, B, "parent/First");
            w.parent(B, C, "parent/Second");
            if prime == "true" {
                assert_eq!(w.value(A, "K"), "A");
                assert_eq!(w.value(C, "K"), "C");
            }
            let mut edges = [(B, "child/First"), (C, "child/Second")];
            if reverse == "true" {
                edges.reverse();
            }
            for ((loader, name), expected) in edges.into_iter().zip([first, second]) {
                assert_eq!(w.prepare(loader, name), expected);
            }
            w.delegate_value(B, "K", if reverse == "true" { C } else { A });
            assert_eq!(
                [A, B, C].map(|loader| w.value(loader, "K")).join("/"),
                values
            );
        }
        [kind, expected, pair, repair, "initialize"] => {
            w.parent(A, B, &format!("parent/{kind}"));
            assert_eq!(w.value(A, "Q"), "A");
            assert_eq!(w.value(B, "Q"), "B");
            assert_eq!(w.prepare(B, &format!("child/{kind}")), expected);
            assert_eq!(format!("{}/{}", w.value(A, "P"), w.value(B, "P")), pair);
            w.delegate_value(B, "P", A);
            assert_eq!(w.value(B, "P"), repair);
        }
        _ => panic!("invalid transitive row"),
    }
}

#[test]
fn preparation_and_subsequent_loads_match_jdk_stages() {
    let mut count = 0;
    for line in ROWS.lines() {
        let mut w = World::new("classes");
        let columns: Vec<_> = line.split('\t').collect();
        match columns.as_slice() {
            ["timing", row @ ..] => timing(&mut w, row),
            ["selection", row @ ..] => selection(&mut w, row),
            ["transitive", row @ ..] => transitive(&mut w, row),
            ["packages", kind, "initialize", expected] => {
                let root = if *kind == "Bridge" {
                    w.define(A, "shared/Hidden").unwrap();
                    w.parent(A, B, "shared/Bridge");
                    "other/Leaf".to_owned()
                } else {
                    w.parent(A, B, &format!("shared/Base{kind}"));
                    format!("shared/Child{kind}")
                };
                assert_eq!([w.value(A, "K"), w.value(B, "K")], ["A", "B"]);
                assert_eq!(w.prepare(B, &root), *expected);
            }
            [
                "inherited",
                "distinct-child",
                owners,
                "initialize",
                expected,
            ] => {
                w.parent(A, B, "parent/Impl");
                w.parent(C, B, "iface/Service");
                w.delegate_value(C, "K", A);
                assert_eq!(
                    [A, B, C].map(|loader| w.value(loader, "K")).join("/"),
                    *owners
                );
                assert_eq!(w.prepare(B, "child/Inherit"), *expected);
            }
            [
                "method-order",
                kind,
                "initialize",
                outcome,
                owner,
                "{B,LinkageError}",
            ] => {
                w.parent(A, B, &format!("parent/{kind}"));
                assert_eq!([w.value(A, "Q"), w.value(B, "Q")], ["A", "B"]);
                assert_eq!(w.prepare(B, &format!("child/{kind}")), *outcome);
                assert_eq!(w.value(A, "P"), *owner);
                // CrabSpot promises classfile order even when the JDK permits either prefix.
                assert_eq!(
                    w.value(B, "P"),
                    if kind.ends_with('0') {
                        "LinkageError"
                    } else {
                        "B"
                    }
                );
            }
            _ => panic!("unrecognized loading reference row {line}"),
        }
        count += 1;
    }
    assert_eq!(count, 85);
}

#[test]
fn selection_constraints_follow_se21_even_where_hotspot_permits_split_definitions() {
    let contracts =
        include_str!("../../../tests/fixtures/loading-constraints/selection-contract.tsv");
    let mut count = 0;
    for row in contracts.lines() {
        let columns: Vec<_> = row.split('\t').collect();
        let [kind, left_name, "C", relation] = columns.as_slice() else {
            panic!("bad contract row {row}")
        };
        let equal = match *relation {
            "equal" => true,
            "independent" => false,
            _ => panic!("bad relation: {relation}"),
        };
        for mode in ["split", "late", "shared"] {
            let mut w = World::new("selection-classes");
            w.parent(C, B, "iface/Service");
            let left = match *kind {
                "Suppress" => {
                    w.define(A, "iface/Default").unwrap();
                    w.parent(A, B, "iface/Reabstract");
                    A
                }
                "RootInterface" => {
                    w.parent(A, B, "iface/Default");
                    A
                }
                "DeclaredInterface" => B,
                _ => {
                    w.parent(A, B, &format!("parent/{kind}"));
                    A
                }
            };
            assert_eq!(NAMES[left], *left_name);
            if mode == "shared" {
                w.delegate_value(C, "K", left);
            }
            if mode != "late" {
                assert_eq!(w.value(left, "K"), NAMES[left]);
                assert_eq!(
                    w.value(C, "K"),
                    if mode == "shared" { NAMES[left] } else { "C" }
                );
            }
            let conflict = equal && mode == "split";
            assert_eq!(
                w.prepare(B, &format!("child/{kind}")),
                if conflict { "LinkageError" } else { "ok" },
                "{row} {mode}"
            );
            assert_eq!(
                w.domain
                    .context(Default::default())
                    .constrained(&units("value/K"), w.loaders[left], w.loaders[C])
                    .unwrap(),
                equal && !conflict
            );
            assert_eq!(w.value(left, "K"), NAMES[left]);
            let after = match mode {
                "late" if equal => "LinkageError",
                "shared" => NAMES[left],
                _ => "C",
            };
            assert_eq!(w.value(C, "K"), after, "{row} {mode}");
            count += 1;
        }
    }
    assert_eq!(count, 27);
}
