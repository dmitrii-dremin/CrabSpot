//! Fixed checked facts, bound to an actual decoded operation before scheduling.
use super::*;
use crabspot_runtime::{ArrayOperandFault, CastOperandFault, OperandExceptionFault};
#[derive(Clone, Copy, Debug)]
pub(super) struct OperandEvent {
    pub fault: OperandExceptionFault,
    opcode: u8,
    dimensions: u8,
}
pub(super) fn operand_kind(fault: OperandExceptionFault) -> VmExceptionKind {
    match fault {
        OperandExceptionFault::Bounds { .. } => VmExceptionKind::ArrayIndexOutOfBoundsException,
        OperandExceptionFault::NegativeSize { .. } => VmExceptionKind::NegativeArraySizeException,
        OperandExceptionFault::Store { .. } => VmExceptionKind::ArrayStoreException,
        OperandExceptionFault::Cast { .. } => VmExceptionKind::ClassCastException,
    }
}
fn valid_event(event: &OperandEvent) -> bool {
    match event.fault {
        OperandExceptionFault::Bounds { index, length } => {
            matches!(event.opcode,0x2e..=0x35|0x4f..=0x56)
                && event.dimensions == 0
                && length >= 0
                && (index < 0 || index >= length)
        }
        OperandExceptionFault::NegativeSize { length, dimension } => {
            length < 0
                && match event.opcode {
                    0xbc | 0xbd => dimension == 0 && event.dimensions == 0,
                    0xc5 => event.dimensions > 0 && dimension < event.dimensions,
                    _ => false,
                }
        }
        OperandExceptionFault::Store { .. } => event.opcode == 0x53 && event.dimensions == 0,
        OperandExceptionFault::Cast { .. } => event.opcode == 0xc0 && event.dimensions == 0,
    }
}
fn event(
    fault: OperandExceptionFault,
    opcode: u8,
    dimensions: u8,
    pc: u32,
) -> Result<Action, ExecutionError> {
    let event = OperandEvent {
        fault,
        opcode,
        dimensions,
    };
    if !valid_event(&event) {
        return Err(ExecutionError::new(
            Some(pc),
            E::VmExceptionProtocol {
                reason: "operand fault does not match operation",
            },
        ));
    }
    Ok(Action::OperandFault(event))
}
pub(super) fn array_operand(
    fault: ArrayOperandFault,
    opcode: u8,
    dimensions: u8,
    pc: u32,
) -> Result<Action, ExecutionError> {
    let fault = match fault {
        ArrayOperandFault::Bounds { index, length } => {
            OperandExceptionFault::Bounds { index, length }
        }
        ArrayOperandFault::NegativeSize { length, dimension } => {
            OperandExceptionFault::NegativeSize { length, dimension }
        }
        ArrayOperandFault::Store { source, component } => {
            OperandExceptionFault::Store { source, component }
        }
    };
    // aaload never reports a store fact; every other allowed variant is operation-bound.
    if opcode == 0x32 && !matches!(fault, OperandExceptionFault::Bounds { .. }) {
        return Err(operand_error(pc));
    }
    event(fault, opcode, dimensions, pc)
}
pub(super) fn cast_operand(fault: CastOperandFault, pc: u32) -> Result<Action, ExecutionError> {
    let CastOperandFault::Mismatch { source, target } = fault;
    event(OperandExceptionFault::Cast { source, target }, 0xc0, 0, pc)
}
pub(super) fn primitive_operand(
    error: HeapError,
    opcode: u8,
    pc: u32,
) -> Result<Action, ExecutionError> {
    match error {
        HeapError::IndexOutOfBounds { index, length } => event(
            OperandExceptionFault::Bounds { index, length },
            opcode,
            0,
            pc,
        ),
        HeapError::NegativeLength { length } => event(
            OperandExceptionFault::NegativeSize {
                length,
                dimension: 0,
            },
            opcode,
            0,
            pc,
        ),
        error => heap_failure(error, pc),
    }
}
pub(super) fn verify_operand_event(
    event: &OperandEvent,
    java: &JavaState,
    current: MethodKey,
    pc: u32,
) -> Result<(), ExecutionError> {
    let instruction = java
        .cache
        .get(&current)
        .and_then(|m| m.decoded.instruction_at(pc))
        .ok_or_else(|| operand_error(pc))?;
    let dimensions = match &instruction.operands {
        Operands::MultiANewArray { dimensions, .. } => *dimensions,
        _ => 0,
    };
    if !valid_event(event) || instruction.opcode != event.opcode || dimensions != event.dimensions {
        return Err(ExecutionError::new(
            Some(pc),
            E::VmExceptionProtocol {
                reason: "operand event differs from executing instruction",
            },
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    mod world {
        pub use crate::execute::assertion_tests::world::recipes;
        include!("../tests/support/operand_world_impl.rs");
    }
    #[test]
    fn decoded_operation_and_requested_dimension_guard_reject_forged_event_history() {
        let mut c = world::app();
        let ty = c.class("[[I");
        c.method(
            "m",
            "()V",
            9,
            &[
                vec![3, 2, 0xc5],
                ty.to_be_bytes().to_vec(),
                vec![2, 0x57, 0xb1],
            ]
            .concat(),
            2,
            0,
        );
        let mut w = world::World::new(c);
        let m = w.method("m", "()V");
        let mut context = w.context();
        let mut code = 0;
        let p = prepare_method_with_context(
            context.registry().class(m.owner()).unwrap(),
            m.index(),
            &mut code,
            usize::MAX,
            Preparation::Static,
            Some((&mut context, m, true)),
            None,
        )
        .unwrap();
        let java = JavaState {
            cache: HashMap::from([(MethodKey::Registered(m), p)]),
            frames: vec![],
            reserved: 0,
            used_code_bytes: code,
            remaining: 1,
            monitors: None,
            vm: None,
        };
        let valid = OperandEvent {
            fault: OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 1,
            },
            opcode: 0xc5,
            dimensions: 2,
        };
        assert!(verify_operand_event(&valid, &java, MethodKey::Registered(m), 2).is_ok());
        for (event, pc) in [
            (
                OperandEvent {
                    dimensions: 3,
                    ..valid
                },
                2,
            ),
            (
                OperandEvent {
                    opcode: 0xbd,
                    dimensions: 0,
                    fault: OperandExceptionFault::NegativeSize {
                        length: -1,
                        dimension: 0,
                    },
                },
                2,
            ),
            (valid, 1),
            (valid, 3),
        ] {
            let e = verify_operand_event(&event, &java, MethodKey::Registered(m), pc).unwrap_err();
            assert_eq!(e.offset, Some(pc));
        }
        assert!(java.frames.is_empty());
        assert_eq!(java.remaining, 1);
    }
    #[test]
    fn forged_opcode_dimension_or_in_range_bounds_never_become_materialization() {
        for (fault, opcode, dimensions) in [
            (
                OperandExceptionFault::Bounds {
                    index: 0,
                    length: 1,
                },
                0x32,
                0,
            ),
            (
                OperandExceptionFault::Bounds {
                    index: -1,
                    length: -1,
                },
                0x32,
                0,
            ),
            (
                OperandExceptionFault::Bounds {
                    index: -1,
                    length: 1,
                },
                0xbc,
                0,
            ),
            (
                OperandExceptionFault::Bounds {
                    index: -1,
                    length: 1,
                },
                0x32,
                1,
            ),
            (
                OperandExceptionFault::NegativeSize {
                    length: 0,
                    dimension: 0,
                },
                0xbd,
                0,
            ),
            (
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 1,
                },
                0xbd,
                0,
            ),
            (
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 2,
                },
                0xc5,
                2,
            ),
            (
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 0,
                },
                0xc5,
                0,
            ),
            (
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 0,
                },
                0x53,
                0,
            ),
        ] {
            let e = event(fault, opcode, dimensions, 17).err().unwrap();
            assert_eq!(e.offset, Some(17));
            assert!(matches!(e.kind, E::VmExceptionProtocol { .. }));
        }
        assert!(
            event(
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 254
                },
                0xc5,
                255,
                17
            )
            .is_ok()
        );
    }
    #[test]
    fn wrong_detailed_variant_is_host_and_nested_heap_failures_are_not_projected() {
        assert!(
            array_operand(
                ArrayOperandFault::NegativeSize {
                    length: -1,
                    dimension: 0
                },
                0x32,
                0,
                7
            )
            .is_err()
        );
        assert!(
            array_operand(
                ArrayOperandFault::Bounds {
                    index: -1,
                    length: 1
                },
                0xbd,
                0,
                7
            )
            .is_err()
        );
        for error in [HeapError::ReadOnlyStringBacking, HeapError::NullReference] {
            let action = primitive_operand(error.clone(), 0x54, 7);
            if error == HeapError::ReadOnlyStringBacking {
                let e = action.err().unwrap();
                assert_eq!(e.kind, E::Heap(error));
            } else {
                assert!(matches!(action.unwrap(), Action::NullPointerException));
            }
        }
    }
}
