//! Same-supplied-class body calls, independent of Java linkage and initialization.
use crabspot_classfile::{AttributeBody, ClassFile, parse};
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome, Value,
    execute_static_method_body, execute_static_method_body_with_limits,
};

fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}

struct Method {
    flags: u16,
    name: u16,
    descriptor: u16,
    code: Option<(Vec<u8>, u16, u16)>,
}

struct Builder {
    pool: Vec<Vec<u8>>,
    owner: u16,
    superclass: u16,
    code_name: u16,
    flags: u16,
    methods: Vec<Method>,
}

impl Builder {
    fn new(interface: bool) -> Self {
        let mut builder = Self {
            pool: Vec::new(),
            owner: 0,
            superclass: 0,
            code_name: 0,
            flags: if interface { 0x601 } else { 0x21 },
            methods: Vec::new(),
        };
        builder.owner = builder.named_class("example/Calls");
        builder.superclass = builder.named_class("java/lang/Object");
        builder.code_name = builder.utf8("Code");
        builder
    }

    fn constant(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let index = (self.pool.len() + 1) as u16;
        let mut bytes = vec![tag];
        bytes.extend(payload);
        self.pool.push(bytes);
        index
    }

    fn utf8(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut bytes = words(&[text.len() as u16]);
        bytes.extend(text.bytes());
        self.constant(1, &bytes)
    }

    fn named_class(&mut self, name: &str) -> u16 {
        let index = self.utf8(name);
        self.constant(7, &words(&[index]))
    }

    fn reference(&mut self, tag: u8, owner: u16, name: &str, descriptor: &str) -> u16 {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let name_and_type = self.constant(12, &words(&[name, descriptor]));
        self.constant(tag, &words(&[owner, name_and_type]))
    }

    fn method(
        &mut self,
        name: &str,
        descriptor: &str,
        code: &[u8],
        stack: u16,
        locals: u16,
    ) -> usize {
        self.declaration(name, descriptor, 9, Some((code.to_vec(), stack, locals)))
    }

    fn declaration(
        &mut self,
        name: &str,
        descriptor: &str,
        flags: u16,
        code: Option<(Vec<u8>, u16, u16)>,
    ) -> usize {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let index = self.methods.len();
        self.methods.push(Method {
            flags,
            name,
            descriptor,
            code,
        });
        index
    }

    fn finish(self) -> ClassFile {
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        bytes.extend(words(&[(self.pool.len() + 1) as u16]));
        bytes.extend(self.pool.into_iter().flatten());
        bytes.extend(words(&[
            self.flags,
            self.owner,
            self.superclass,
            0,
            0,
            self.methods.len() as u16,
        ]));
        for method in self.methods {
            bytes.extend(words(&[
                method.flags,
                method.name,
                method.descriptor,
                u16::from(method.code.is_some()),
            ]));
            if let Some((code, stack, locals)) = method.code {
                bytes.extend(words(&[self.code_name]));
                bytes.extend((12 + code.len() as u32).to_be_bytes());
                bytes.extend(words(&[stack, locals]));
                bytes.extend((code.len() as u32).to_be_bytes());
                bytes.extend(code);
                bytes.extend(words(&[0, 0]));
            }
        }
        bytes.extend(words(&[0]));
        parse(&bytes).unwrap()
    }
}

fn invoke(index: u16) -> Vec<u8> {
    let mut code = vec![0xb8];
    code.extend(index.to_be_bytes());
    code
}
fn execute(class: &ClassFile, args: &[Value]) -> Result<ExecutionOutcome, ExecutionError> {
    execute_static_method_body(class, 0, args, ExecutionOptions::default())
}
fn limits(
    class: &ClassFile,
    fuel: u64,
    limits: CallLimits,
) -> Result<ExecutionOutcome, ExecutionError> {
    execute_static_method_body_with_limits(
        class,
        0,
        &[],
        ExecutionOptions {
            instruction_budget: fuel,
        },
        limits,
    )
}
fn returned(value: Value) -> ExecutionOutcome {
    ExecutionOutcome::Returned(Some(value))
}
fn simple_chain() -> ClassFile {
    let mut builder = Builder::new(false);
    let reference = builder.reference(10, builder.owner, "leaf", "()I");
    let mut code = invoke(reference);
    code.push(0xac);
    builder.method("root", "()I", &code, 1, 0);
    builder.method("leaf", "()I", &[0x04, 0xac], 1, 0);
    builder.finish()
}

#[test]
fn call_preserves_caller_operands_argument_order_and_descriptor_overloads() {
    let mut builder = Builder::new(false);
    let alias = builder.named_class("example/Calls");
    let reference = builder.reference(10, alias, "pick", "(II)I");
    let mut code = vec![0x10, 100, 0x05, 0x10, 7];
    code.extend(invoke(reference));
    code.extend([0x60, 0xac]);
    builder.method("root", "()I", &code, 3, 0);
    builder.method("pick", "(J)J", &[0x1e, 0xad], 2, 2);
    builder.method(
        "pick",
        "(II)I",
        &[0x1a, 0x10, 10, 0x68, 0x1b, 0x60, 0xac],
        2,
        2,
    );
    assert_eq!(
        execute(&builder.finish(), &[]).unwrap(),
        returned(Value::Int(127))
    );
}

#[test]
fn mixed_width_arguments_reach_the_callee_in_descriptor_order() {
    let mut builder = Builder::new(false);
    let reference = builder.reference(10, builder.owner, "sum", "(IFJD)D");
    let mut code = vec![0x1a, 0x23, 0x20, 0x18, 4];
    code.extend(invoke(reference));
    code.push(0xaf);
    builder.method("root", "(IFJD)D", &code, 6, 6);
    builder.method(
        "sum",
        "(IFJD)D",
        &[
            0x18, 4, 0x23, 0x8d, 0x63, 0x20, 0x8a, 0x63, 0x1a, 0x87, 0x63, 0xaf,
        ],
        4,
        6,
    );
    assert_eq!(
        execute(
            &builder.finish(),
            &[
                Value::Int(2),
                Value::Float(0x3fc0_0000),
                Value::Long(3),
                Value::Double(0x401c_0000_0000_0000)
            ]
        )
        .unwrap(),
        returned(Value::Double(0x402b_0000_0000_0000))
    );
}

#[test]
fn return_transfer_preserves_numeric_bits_and_narrows_integer_types() {
    for (descriptor, argument, load, ret) in [
        ("(J)J", Value::Long(i64::MIN + 17), 0x1e, 0xad),
        ("(F)F", Value::Float(0xffa0_1234), 0x22, 0xae),
        ("(D)D", Value::Double(0xfff4_0000_0000_5678), 0x26, 0xaf),
    ] {
        let mut builder = Builder::new(false);
        let reference = builder.reference(10, builder.owner, "identity", descriptor);
        let mut code = vec![load];
        code.extend(invoke(reference));
        code.push(ret);
        builder.method("root", descriptor, &code, 2, 2);
        builder.method("identity", descriptor, &[load, ret], 2, 2);
        assert_eq!(
            execute(&builder.finish(), &[argument]).unwrap(),
            returned(argument)
        );
    }
    for (descriptor, code, expected) in [
        ("()B", vec![0x11, 0, 128, 0xac], -128),
        ("()C", vec![0x02, 0xac], 65535),
        ("()S", vec![0x11, 128, 0, 0xac], -32768),
        ("()Z", vec![0x06, 0xac], 1),
    ] {
        let mut builder = Builder::new(false);
        let reference = builder.reference(10, builder.owner, "narrow", descriptor);
        let mut caller = invoke(reference);
        caller.push(0xac);
        builder.method("root", "()I", &caller, 1, 0);
        builder.method("narrow", descriptor, &code, 1, 0);
        assert_eq!(
            execute(&builder.finish(), &[]).unwrap(),
            returned(Value::Int(expected))
        );
    }
}

#[test]
fn void_return_transfers_nothing_and_discards_the_callee_residual_stack() {
    let mut builder = Builder::new(false);
    let reference = builder.reference(10, builder.owner, "side", "()V");
    let mut code = vec![0x10, 100];
    code.extend(invoke(reference));
    code.push(0xac);
    builder.method("root", "()I", &code, 1, 0);
    builder.method("side", "()V", &[0x04, 0xb1], 1, 0);
    assert_eq!(
        execute(&builder.finish(), &[]).unwrap(),
        returned(Value::Int(100))
    );
}

#[test]
fn same_interface_calls_require_interface_reference_and_supported_version() {
    let mut builder = Builder::new(true);
    let alias = builder.named_class("example/Calls");
    let reference = builder.reference(11, alias, "leaf", "()I");
    let mut code = invoke(reference);
    code.push(0xac);
    builder.method("root", "()I", &code, 1, 0);
    builder.method("leaf", "()I", &[0x05, 0xac], 1, 0);
    let mut class = builder.finish();
    class.version.major = 52;
    assert_eq!(execute(&class, &[]).unwrap(), returned(Value::Int(2)));
    // Public metadata can be supplied after mutation; execution still checks
    // the call opcode/reference's interface version boundary independently.
    class.version.major = 51;
    assert!(matches!(
        execute(&class, &[]).unwrap_err().kind,
        E::InvalidCallReference { .. }
    ));
    for interface in [false, true] {
        let mut builder = Builder::new(interface);
        let reference = builder.reference(
            if interface { 10 } else { 11 },
            builder.owner,
            "leaf",
            "()I",
        );
        let mut code = invoke(reference);
        code.push(0xac);
        builder.method("root", "()I", &code, 1, 0);
        builder.method("leaf", "()I", &[0x04, 0xac], 1, 0);
        let error = execute(&builder.finish(), &[]).unwrap_err();
        assert!(matches!(error.kind, E::InvalidCallReference { .. }));
        assert_eq!(error.offset, Some(0));
        assert_eq!(error.method.as_ref().unwrap().index, 0);
    }
}

#[test]
fn malformed_call_tags_missing_targets_and_external_owners_are_explicit_errors() {
    for mode in 0..7 {
        let mut builder = Builder::new(false);
        let reference = match mode {
            0 => builder.utf8("not a method reference"),
            1 => builder.owner,
            2 => builder.constant(3, &1_i32.to_be_bytes()),
            3 => builder.reference(9, builder.owner, "field", "I"),
            4 => builder.reference(10, builder.owner, "missing", "()I"),
            5 => {
                let owner = builder.named_class("external/Owner");
                builder.reference(10, owner, "leaf", "()I")
            }
            _ => u16::MAX,
        };
        let mut code = invoke(reference);
        code.push(0xac);
        builder.method("root", "()I", &code, 1, 0);
        let error = execute(&builder.finish(), &[]).unwrap_err();
        assert!(match mode {
            0..=3 | 6 => matches!(error.kind, E::InvalidCallReference { .. }),
            4 => matches!(error.kind, E::MissingDeclaredMethod),
            _ => matches!(error.kind, E::UnsupportedCall(_)),
        });
        assert_eq!(error.offset, Some(0));
        assert!(error.method.as_ref().unwrap().name.equals_ascii("root"));
    }
}

#[test]
fn unsupported_target_declarations_and_handlers_are_checked_on_entry() {
    for mode in 0..6 {
        let mut builder = Builder::new(false);
        let name = if mode == 5 { "<init>" } else { "target" };
        let reference = builder.reference(10, builder.owner, name, "()V");
        let mut code = vec![0];
        code.extend(invoke(reference));
        code.push(0xb1);
        builder.method("root", "()V", &code, 0, 0);
        let flags = match mode {
            0 | 5 => 1,
            1 => 0x109,
            2 => 0x29,
            3 => 0x401,
            _ => 9,
        };
        if mode == 3 {
            builder.flags |= 0x400;
        }
        let body = if matches!(mode, 1 | 3) {
            None
        } else {
            Some((vec![0xb1], 0, 1))
        };
        builder.declaration(name, "()V", flags, body);
        let mut class = builder.finish();
        if mode == 4 {
            let AttributeBody::Code(code) = &mut class.methods[1].attributes[0].body else {
                panic!("missing target code")
            };
            code.exception_table
                .push(crabspot_classfile::ExceptionHandler {
                    start_pc: 0,
                    end_pc: 1,
                    handler_pc: 0,
                    catch_type: 0,
                });
        }
        let error = execute(&class, &[]).unwrap_err();
        assert!(matches!(
            error.kind,
            E::UnsupportedMethod(_) | E::UnsupportedCall(_)
        ));
        let site = error.call_site.as_ref().unwrap();
        assert_eq!(site.method.index, 0);
        assert_eq!(site.offset, 1);
    }
}

#[test]
fn callee_decode_failure_reports_target_identity_and_preserves_call_site() {
    let mut builder = Builder::new(false);
    let reference = builder.reference(10, builder.owner, "broken", "()V");
    let mut code = vec![0];
    code.extend(invoke(reference));
    code.push(0xb1);
    builder.method("root", "()V", &code, 0, 0);
    // Even the unreachable byte after return is decoded before target entry.
    builder.method("broken", "()V", &[0xb1, 0xca], 0, 0);
    let error = execute(&builder.finish(), &[]).unwrap_err();
    assert!(matches!(error.kind, E::Decode(_)));
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.method.as_ref().unwrap().index, 1);
    assert!(error.method.as_ref().unwrap().name.equals_ascii("broken"));
    assert!(
        error
            .method
            .as_ref()
            .unwrap()
            .descriptor
            .equals_ascii("()V")
    );
    assert_eq!(error.call_site.as_ref().unwrap().method.index, 0);
    assert_eq!(error.call_site.as_ref().unwrap().offset, 1);
}

#[test]
fn call_argument_underflow_wrong_types_and_return_overflow_remain_host_errors() {
    for wrong_type in [false, true] {
        let mut builder = Builder::new(false);
        let reference = builder.reference(10, builder.owner, "target", "(J)J");
        let mut code = if wrong_type { vec![0x03] } else { Vec::new() };
        code.extend(invoke(reference));
        code.push(0xad);
        builder.method("root", "()J", &code, 2, 0);
        builder.method("target", "(J)J", &[0x1e, 0xad], 2, 2);
        let error = execute(&builder.finish(), &[]).unwrap_err();
        assert!(if wrong_type {
            matches!(error.kind, E::WrongValueType { .. })
        } else {
            matches!(error.kind, E::StackUnderflow)
        });
    }
    let mut builder = Builder::new(false);
    let reference = builder.reference(10, builder.owner, "target", "()D");
    let mut code = vec![0x03];
    code.extend(invoke(reference));
    code.push(0xaf);
    builder.method("root", "()D", &code, 2, 0);
    builder.method("target", "()D", &[0x0e, 0xaf], 2, 0);
    let error = execute(&builder.finish(), &[]).unwrap_err();
    assert!(matches!(
        error.kind,
        E::StackOverflow {
            required: 3,
            capacity: 2
        }
    ));
    assert_eq!(error.method.as_ref().unwrap().index, 0);
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.call_site.as_ref().unwrap().offset, 1);
}

#[test]
fn uncaught_arithmetic_exception_in_descendant_stops_all_callers() {
    let mut builder = Builder::new(false);
    let middle = builder.reference(10, builder.owner, "middle", "()I");
    let leaf = builder.reference(10, builder.owner, "leaf", "()I");
    let mut root = invoke(middle);
    root.extend([0x57, 0xbb, 0, 2, 0xac]); // unsupported `new` must not execute
    builder.method("root", "()I", &root, 1, 0);
    let mut code = invoke(leaf);
    code.push(0xac);
    builder.method("middle", "()I", &code, 1, 0);
    builder.method("leaf", "()I", &[0x04, 0x03, 0x6c, 0xac], 2, 0);
    assert_eq!(
        execute(&builder.finish(), &[]).unwrap(),
        ExecutionOutcome::ArithmeticException
    );
}

#[test]
fn shared_instruction_budget_counts_calls_and_returns_exactly_once() {
    let class = simple_chain();
    assert_eq!(
        limits(&class, 4, CallLimits::default()).unwrap(),
        returned(Value::Int(1))
    );
    for (fuel, method, offset) in [(0, 0, 0), (1, 1, 0), (2, 1, 1), (3, 0, 3)] {
        let error = limits(&class, fuel, CallLimits::default()).unwrap_err();
        assert_eq!(error.kind, E::InstructionBudgetExhausted { limit: fuel });
        assert_eq!(error.method.as_ref().unwrap().index, method);
        assert_eq!(error.offset, Some(offset));
    }
}

#[test]
fn frame_and_aggregate_slot_limits_include_root_and_allow_exact_boundaries() {
    let class = simple_chain();
    assert_eq!(
        limits(
            &class,
            4,
            CallLimits {
                max_frames: 2,
                max_frame_slots: 2,
                ..CallLimits::default()
            }
        )
        .unwrap(),
        returned(Value::Int(1))
    );
    for (policy, expected) in [
        (
            CallLimits {
                max_frames: 0,
                ..CallLimits::default()
            },
            E::FrameLimit {
                limit: 0,
                required: 1,
            },
        ),
        (
            CallLimits {
                max_frames: 1,
                ..CallLimits::default()
            },
            E::FrameLimit {
                limit: 1,
                required: 2,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 0,
                ..CallLimits::default()
            },
            E::FrameSlotLimit {
                limit: 0,
                required: 1,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 1,
                ..CallLimits::default()
            },
            E::FrameSlotLimit {
                limit: 1,
                required: 2,
            },
        ),
    ] {
        assert_eq!(limits(&class, 100, policy).unwrap_err().kind, expected);
    }
}

#[test]
fn returned_frames_release_slots_and_repeated_callees_are_cached_once() {
    let mut builder = Builder::new(false);
    let leaf = builder.reference(10, builder.owner, "leaf", "()I");
    let alias_owner = builder.named_class("example/Calls");
    let alias_leaf = builder.reference(10, alias_owner, "leaf", "()I");
    let mut code = invoke(leaf);
    code.push(0x57);
    code.extend(invoke(alias_leaf));
    code.push(0xac);
    builder.method("root", "()I", &code, 1, 0); // 8 code bytes
    builder.method("leaf", "()I", &[0x04, 0xac], 1, 0); // 2 code bytes
    let class = builder.finish();
    let policy = CallLimits {
        max_frames: 2,
        max_frame_slots: 2,
        max_prepared_code_bytes: 10,
    };
    assert_eq!(limits(&class, 8, policy).unwrap(), returned(Value::Int(1)));
    assert_eq!(
        limits(
            &class,
            8,
            CallLimits {
                max_prepared_code_bytes: 9,
                ..policy
            }
        )
        .unwrap_err()
        .kind,
        E::PreparedCodeLimit {
            limit: 9,
            required: 10
        }
    );
    assert_eq!(
        limits(
            &class,
            8,
            CallLimits {
                max_prepared_code_bytes: 0,
                ..policy
            }
        )
        .unwrap_err()
        .kind,
        E::PreparedCodeLimit {
            limit: 0,
            required: 8
        }
    );
}

#[test]
fn prepared_code_budget_retains_distinct_callees_across_returns() {
    let mut builder = Builder::new(false);
    let first = builder.reference(10, builder.owner, "first", "()I");
    let second = builder.reference(10, builder.owner, "second", "()I");
    let mut code = invoke(first);
    code.push(0x57);
    code.extend(invoke(second));
    code.push(0xac);
    builder.method("root", "()I", &code, 1, 0);
    builder.method("first", "()I", &[0x04, 0xac], 1, 0);
    builder.method("second", "()I", &[0x05, 0xac], 1, 0);
    let class = builder.finish();
    assert_eq!(
        limits(
            &class,
            8,
            CallLimits {
                max_prepared_code_bytes: 12,
                ..CallLimits::default()
            }
        )
        .unwrap(),
        returned(Value::Int(2))
    );
    let error = limits(
        &class,
        8,
        CallLimits {
            max_prepared_code_bytes: 11,
            ..CallLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        E::PreparedCodeLimit {
            limit: 11,
            required: 12
        }
    );
    assert_eq!(error.call_site.as_ref().unwrap().offset, 4);
}

#[test]
fn deep_recursion_uses_explicit_frames_and_a_single_tight_fuel_budget() {
    let mut builder = Builder::new(false);
    let recurse = builder.reference(10, builder.owner, "root", "()V");
    let mut code = invoke(recurse);
    code.push(0xb1);
    builder.method("root", "()V", &code, 0, 0);
    let class = builder.finish();
    let policy = CallLimits {
        max_frames: usize::MAX,
        max_frame_slots: 0,
        max_prepared_code_bytes: 4,
    };
    let error = limits(&class, 20_000, policy).unwrap_err();
    assert_eq!(error.kind, E::InstructionBudgetExhausted { limit: 20_000 });
    assert_eq!(error.method.as_ref().unwrap().index, 0);
    assert_eq!(error.offset, Some(0));
    assert_eq!(
        limits(
            &class,
            1000,
            CallLimits {
                max_frames: 50,
                ..policy
            }
        )
        .unwrap_err()
        .kind,
        E::FrameLimit {
            limit: 50,
            required: 51
        }
    );
}

#[test]
fn target_reference_descriptors_missing_code_and_wrong_returns_fail_before_continuation() {
    for mode in 0..3 {
        let mut builder = Builder::new(false);
        let descriptor = if mode == 0 {
            "(Ljava/lang/Object;)I"
        } else {
            "()I"
        };
        let reference = builder.reference(10, builder.owner, "target", descriptor);
        let mut code = invoke(reference);
        code.push(0xac);
        builder.method("root", "()I", &code, 1, 0);
        builder.method(
            "target",
            descriptor,
            if mode == 2 {
                &[0x0b, 0xae]
            } else {
                &[0x04, 0xac]
            },
            1,
            1,
        );
        let mut class = builder.finish();
        if mode == 1 {
            class.methods[1].attributes.clear();
        }
        let error = execute(&class, &[]).unwrap_err();
        assert!(match mode {
            0 => matches!(error.kind, E::UnsupportedDescriptor),
            1 => matches!(error.kind, E::InvalidMethod(_)),
            _ => matches!(error.kind, E::ReturnTypeMismatch),
        });
        assert_eq!(error.method.as_ref().unwrap().index, 1);
        assert_eq!(error.call_site.as_ref().unwrap().method.index, 0);
        assert_eq!(error.call_site.as_ref().unwrap().offset, 0);
    }
}

#[test]
fn aggregate_slot_reservations_include_unused_locals_and_full_stack_capacity() {
    let mut builder = Builder::new(false);
    let leaf = builder.reference(10, builder.owner, "leaf", "()I");
    let mut code = invoke(leaf);
    code.push(0xac);
    builder.method("root", "()I", &code, 5, 7); // reserves 12, despite sparse actual state
    builder.method("leaf", "()I", &[0x04, 0xac], 3, 11); // reserves 14
    let class = builder.finish();
    assert_eq!(
        limits(
            &class,
            4,
            CallLimits {
                max_frame_slots: 26,
                ..CallLimits::default()
            }
        )
        .unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(
        limits(
            &class,
            4,
            CallLimits {
                max_frame_slots: 25,
                ..CallLimits::default()
            }
        )
        .unwrap_err()
        .kind,
        E::FrameSlotLimit {
            limit: 25,
            required: 26
        }
    );
    assert_eq!(
        limits(
            &class,
            4,
            CallLimits {
                max_frame_slots: 11,
                ..CallLimits::default()
            }
        )
        .unwrap_err()
        .kind,
        E::FrameSlotLimit {
            limit: 11,
            required: 12
        }
    );
}
