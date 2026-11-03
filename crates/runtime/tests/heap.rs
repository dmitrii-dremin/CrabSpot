use crabspot_runtime::{
    HeapError, HeapOptions, HeapResource, ManagedHeap, PrimitiveArrayKind as K, Reference, Value,
    ValueKind,
};
use std::collections::HashSet;

const KINDS: [K; 8] = [
    K::Boolean,
    K::Byte,
    K::Char,
    K::Short,
    K::Int,
    K::Long,
    K::Float,
    K::Double,
];
fn heap(options: HeapOptions) -> ManagedHeap {
    ManagedHeap::new(options).unwrap()
}

#[test]
fn every_kind_reports_storage_width_computational_kind_and_zero_defaults() {
    let mut heap = heap(HeapOptions::default());
    let mut identities = HashSet::new();
    for (kind, width, value_kind, default) in [
        (K::Boolean, 1, ValueKind::Int, Value::Int(0)),
        (K::Byte, 1, ValueKind::Int, Value::Int(0)),
        (K::Char, 2, ValueKind::Int, Value::Int(0)),
        (K::Short, 2, ValueKind::Int, Value::Int(0)),
        (K::Int, 4, ValueKind::Int, Value::Int(0)),
        (K::Long, 8, ValueKind::Long, Value::Long(0)),
        (K::Float, 4, ValueKind::Float, Value::Float(0)),
        (K::Double, 8, ValueKind::Double, Value::Double(0)),
    ] {
        assert_eq!(kind.element_bytes(), width);
        assert_eq!(kind.value_kind(), value_kind);
        let reference = heap.allocate_primitive_array(kind, 3).unwrap();
        assert!(!reference.is_null());
        assert!(identities.insert(reference));
        assert_eq!(heap.kind(reference).unwrap(), kind);
        assert_eq!(heap.length(reference).unwrap(), 3);
        heap.validate_root(reference.heap_ref().unwrap()).unwrap();
        for index in 0..3 {
            assert_eq!(heap.load(reference, index).unwrap(), default);
        }
    }
    assert_eq!(heap.allocated_arrays(), 8);
    assert_eq!(heap.total_elements(), 24);
    assert_eq!(heap.payload_bytes(), 90);
}

#[test]
fn integer_computational_stores_use_boolean_masking_and_signed_or_unsigned_narrowing() {
    let mut heap = heap(HeapOptions::default());
    for (kind, cases) in [
        (
            K::Boolean,
            vec![
                (0, 0),
                (1, 1),
                (2, 0),
                (3, 1),
                (-1, 1),
                (-2, 0),
                (i32::MIN, 0),
                (i32::MAX, 1),
            ],
        ),
        (
            K::Byte,
            vec![
                (127, 127),
                (128, -128),
                (255, -1),
                (256, 0),
                (-1, -1),
                (-129, 127),
                (i32::MIN, 0),
                (i32::MAX, -1),
            ],
        ),
        (
            K::Char,
            vec![
                (-1, 65535),
                (65535, 65535),
                (65536, 0),
                (65537, 1),
                (i32::MIN, 0),
                (i32::MAX, 65535),
            ],
        ),
        (
            K::Short,
            vec![
                (32767, 32767),
                (32768, -32768),
                (65535, -1),
                (65536, 0),
                (-1, -1),
                (-32769, 32767),
                (i32::MIN, 0),
                (i32::MAX, -1),
            ],
        ),
        (
            K::Int,
            vec![(i32::MIN, i32::MIN), (-1, -1), (0, 0), (i32::MAX, i32::MAX)],
        ),
    ] {
        let reference = heap.allocate_primitive_array(kind, 1).unwrap();
        for (input, expected) in cases {
            heap.store(reference, 0, Value::Int(input)).unwrap();
            assert_eq!(
                heap.load(reference, 0).unwrap(),
                Value::Int(expected),
                "{kind:?} <- {input}"
            );
        }
    }
}

#[test]
fn long_and_floating_elements_round_trip_raw_payloads_including_signed_zero_and_nans() {
    let mut heap = heap(HeapOptions::default());
    let long = heap.allocate_primitive_array(K::Long, 1).unwrap();
    for value in [i64::MIN, i64::MAX, -1, 0, 0x1234_5678_9abc_def0] {
        heap.store(long, 0, Value::Long(value)).unwrap();
        assert_eq!(heap.load(long, 0).unwrap(), Value::Long(value));
    }
    let float = heap.allocate_primitive_array(K::Float, 1).unwrap();
    for bits in [
        0_u32,
        0x8000_0000,
        1,
        0x007f_ffff,
        0x0080_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0xff80_0000,
        0x7fa0_1234,
        0xffc0_5678,
    ] {
        heap.store(float, 0, Value::Float(bits)).unwrap();
        assert_eq!(heap.load(float, 0).unwrap(), Value::Float(bits));
    }
    let double = heap.allocate_primitive_array(K::Double, 1).unwrap();
    for bits in [
        0_u64,
        0x8000_0000_0000_0000,
        1,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x7fef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff4_0000_0000_1234,
        0xfff8_0000_0000_5678,
    ] {
        heap.store(double, 0, Value::Double(bits)).unwrap();
        assert_eq!(heap.load(double, 0).unwrap(), Value::Double(bits));
    }
}

#[test]
fn empty_arrays_have_distinct_identities_and_consume_only_the_array_entry_quota() {
    let mut heap = heap(HeapOptions {
        max_arrays: 8,
        max_array_length: 0,
        max_total_elements: 0,
        max_payload_bytes: 0,
    });
    let mut identities = HashSet::new();
    for kind in KINDS {
        let reference = heap.allocate_primitive_array(kind, 0).unwrap();
        assert!(identities.insert(reference));
        assert_eq!(heap.length(reference).unwrap(), 0);
        assert_eq!(heap.kind(reference).unwrap(), kind);
        assert!(heap.load(reference, 0).is_err());
    }
    assert_eq!(heap.allocated_arrays(), 8);
    assert_eq!(heap.total_elements(), 0);
    assert_eq!(heap.payload_bytes(), 0);
    assert!(heap.allocate_primitive_array(K::Int, 0).is_err());
}

#[test]
fn copied_aliases_survive_arena_growth_and_moving_the_heap_value() {
    let mut original = heap(HeapOptions::default());
    let reference = original.allocate_primitive_array(K::Int, 1).unwrap();
    let alias = reference;
    let handle = reference.heap_ref().unwrap();
    original.store(reference, 0, Value::Int(123)).unwrap();
    for _ in 0..4095 {
        original.allocate_primitive_array(K::Byte, 0).unwrap();
    }
    assert_eq!(original.allocated_arrays(), 4096);
    assert_eq!(original.total_elements(), 1);
    assert_eq!(original.payload_bytes(), 4);
    let boxed = Box::new(original);
    assert_eq!(boxed.load(alias, 0).unwrap(), Value::Int(123));
    let mut moved = *boxed;
    moved.validate_root(handle).unwrap();
    assert_eq!(reference, alias);
    moved.store(alias, 0, Value::Int(-17)).unwrap();
    assert_eq!(moved.load(reference, 0).unwrap(), Value::Int(-17));
    assert_eq!(reference.heap_ref(), Some(handle));
}

#[test]
fn references_follow_heap_identity_when_two_heap_values_are_swapped() {
    let mut first = heap(HeapOptions::default());
    let mut second = heap(HeapOptions::default());
    let first_reference = first.allocate_primitive_array(K::Int, 1).unwrap();
    let second_reference = second.allocate_primitive_array(K::Int, 1).unwrap();
    assert_ne!(first_reference, second_reference);
    first.store(first_reference, 0, Value::Int(1)).unwrap();
    second.store(second_reference, 0, Value::Int(2)).unwrap();
    std::mem::swap(&mut first, &mut second);
    assert_eq!(first.load(second_reference, 0).unwrap(), Value::Int(2));
    assert_eq!(second.load(first_reference, 0).unwrap(), Value::Int(1));
    assert!(first.load(first_reference, 0).is_err());
    assert!(second.load(second_reference, 0).is_err());
}

fn counters(heap: &ManagedHeap) -> (usize, usize, usize) {
    (
        heap.allocated_arrays(),
        heap.total_elements(),
        heap.payload_bytes(),
    )
}

#[test]
fn null_foreign_and_dropped_heap_references_are_structured_failures() {
    let mut heap = heap(HeapOptions::default());
    let null = Reference::null();
    assert_eq!(heap.kind(null).unwrap_err(), HeapError::NullReference);
    assert_eq!(heap.length(null).unwrap_err(), HeapError::NullReference);
    assert_eq!(heap.load(null, 0).unwrap_err(), HeapError::NullReference);
    assert_eq!(
        heap.store(null, 0, Value::Int(1)).unwrap_err(),
        HeapError::NullReference
    );
    assert!(null.heap_ref().is_none());
    let mut foreign_heap = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign = foreign_heap.allocate_primitive_array(K::Int, 1).unwrap();
    let dropped = {
        let mut temporary = ManagedHeap::new(HeapOptions::default()).unwrap();
        temporary.allocate_primitive_array(K::Int, 1).unwrap()
    };
    let mut replacement = ManagedHeap::new(HeapOptions::default()).unwrap();
    let replacement_reference = replacement.allocate_primitive_array(K::Int, 1).unwrap();
    assert_ne!(dropped, replacement_reference);
    assert_eq!(
        replacement.kind(dropped).unwrap_err(),
        HeapError::ForeignHeap
    );
    assert_eq!(
        replacement
            .validate_root(dropped.heap_ref().unwrap())
            .unwrap_err(),
        HeapError::ForeignHeap
    );
    let live = heap.allocate_primitive_array(K::Int, 1).unwrap();
    heap.store(live, 0, Value::Int(17)).unwrap();
    for reference in [foreign, dropped] {
        assert_ne!(reference, live);
        assert_eq!(heap.kind(reference).unwrap_err(), HeapError::ForeignHeap);
        assert_eq!(heap.length(reference).unwrap_err(), HeapError::ForeignHeap);
        assert_eq!(heap.load(reference, 0).unwrap_err(), HeapError::ForeignHeap);
        assert_eq!(
            heap.store(reference, 0, Value::Int(9)).unwrap_err(),
            HeapError::ForeignHeap
        );
        assert_eq!(
            heap.validate_root(reference.heap_ref().unwrap())
                .unwrap_err(),
            HeapError::ForeignHeap
        );
    }
    assert_eq!(heap.load(live, 0).unwrap(), Value::Int(17));
    heap.validate_root(live.heap_ref().unwrap()).unwrap();
    assert_eq!(counters(&heap), (1, 1, 4));
}

#[test]
fn signed_lengths_and_indices_are_checked_and_failed_accesses_leave_elements_unchanged() {
    let mut heap = heap(HeapOptions::default());
    let reference = heap.allocate_primitive_array(K::Int, 1).unwrap();
    heap.store(reference, 0, Value::Int(123)).unwrap();
    let before = counters(&heap);
    for length in [i32::MIN, -1] {
        assert_eq!(
            heap.allocate_primitive_array(K::Byte, length).unwrap_err(),
            HeapError::NegativeLength { length }
        );
        assert_eq!(counters(&heap), before);
    }
    for index in [i32::MIN, -1, 1, i32::MAX] {
        assert_eq!(
            heap.load(reference, index).unwrap_err(),
            HeapError::IndexOutOfBounds { index, length: 1 }
        );
        assert_eq!(
            heap.store(reference, index, Value::Int(9)).unwrap_err(),
            HeapError::IndexOutOfBounds { index, length: 1 }
        );
        assert_eq!(heap.load(reference, 0).unwrap(), Value::Int(123));
        assert_eq!(counters(&heap), before);
    }
    let recovered = heap.allocate_primitive_array(K::Byte, 2).unwrap();
    assert_eq!(heap.length(recovered).unwrap(), 2);
    assert_eq!(heap.load(reference, 0).unwrap(), Value::Int(123));
    let empty = heap.allocate_primitive_array(K::Double, 0).unwrap();
    assert_eq!(
        heap.load(empty, 0).unwrap_err(),
        HeapError::IndexOutOfBounds {
            index: 0,
            length: 0
        }
    );
}

#[test]
fn every_array_kind_rejects_other_value_kinds_without_mutation() {
    let mut heap = heap(HeapOptions::default());
    for kind in KINDS {
        let reference = heap.allocate_primitive_array(kind, 1).unwrap();
        let baseline = match kind {
            K::Boolean => Value::Int(1),
            K::Byte | K::Char | K::Short | K::Int => Value::Int(73),
            K::Long => Value::Long(73),
            K::Float => Value::Float(0x3fc0_0000),
            K::Double => Value::Double(0x3ff8_0000_0000_0000),
        };
        heap.store(reference, 0, baseline).unwrap();
        let before = counters(&heap);
        for value in [
            Value::Int(9),
            Value::Long(9),
            Value::Float(9),
            Value::Double(9),
            Value::Reference(Reference::null()),
            Value::Reference(reference),
        ] {
            if value.kind() == kind.value_kind() {
                continue;
            }
            assert_eq!(
                heap.store(reference, 0, value).unwrap_err(),
                HeapError::ValueKindMismatch {
                    expected: kind.value_kind(),
                    actual: value.kind()
                }
            );
            assert_eq!(heap.load(reference, 0).unwrap(), baseline);
            assert_eq!(counters(&heap), before);
        }
    }
}

#[test]
fn default_options_and_each_zero_quota_are_explicit() {
    let defaults = HeapOptions::default();
    assert_eq!(defaults.max_arrays, 4096);
    assert_eq!(defaults.max_array_length, 1_048_576);
    assert_eq!(defaults.max_total_elements, 4_194_304);
    assert_eq!(defaults.max_payload_bytes, 33_554_432);
    for (options, kind, length, resource) in [
        (
            HeapOptions {
                max_arrays: 0,
                ..HeapOptions::default()
            },
            K::Int,
            0,
            HeapResource::Arrays,
        ),
        (
            HeapOptions {
                max_array_length: 0,
                ..HeapOptions::default()
            },
            K::Int,
            1,
            HeapResource::ArrayLength,
        ),
        (
            HeapOptions {
                max_total_elements: 0,
                ..HeapOptions::default()
            },
            K::Byte,
            1,
            HeapResource::TotalElements,
        ),
        (
            HeapOptions {
                max_payload_bytes: 0,
                ..HeapOptions::default()
            },
            K::Byte,
            1,
            HeapResource::PayloadBytes,
        ),
    ] {
        let mut heap = heap(options);
        assert_eq!(
            heap.allocate_primitive_array(kind, length).unwrap_err(),
            HeapError::ResourceLimit {
                resource,
                limit: 0,
                required: 1
            }
        );
        assert_eq!(counters(&heap), (0, 0, 0));
    }
}

#[test]
fn array_entry_and_length_quotas_accept_exact_boundaries_and_recover_after_rejection() {
    let mut arrays = heap(HeapOptions {
        max_arrays: 2,
        ..HeapOptions::default()
    });
    let empty = arrays.allocate_primitive_array(K::Byte, 0).unwrap();
    let value = arrays.allocate_primitive_array(K::Int, 1).unwrap();
    arrays.store(value, 0, Value::Int(73)).unwrap();
    assert_eq!(
        arrays.allocate_primitive_array(K::Long, 0).unwrap_err(),
        HeapError::ResourceLimit {
            resource: HeapResource::Arrays,
            limit: 2,
            required: 3
        }
    );
    assert_eq!(counters(&arrays), (2, 1, 4));
    assert_eq!(arrays.length(empty).unwrap(), 0);
    assert_eq!(arrays.load(value, 0).unwrap(), Value::Int(73));
    let mut lengths = heap(HeapOptions {
        max_array_length: 3,
        ..HeapOptions::default()
    });
    for kind in KINDS {
        let before = counters(&lengths);
        assert_eq!(
            lengths.allocate_primitive_array(kind, 4).unwrap_err(),
            HeapError::ResourceLimit {
                resource: HeapResource::ArrayLength,
                limit: 3,
                required: 4
            }
        );
        assert_eq!(counters(&lengths), before);
        let reference = lengths.allocate_primitive_array(kind, 3).unwrap();
        assert_eq!(lengths.length(reference).unwrap(), 3);
    }
    assert_eq!(
        lengths
            .allocate_primitive_array(K::Double, i32::MAX)
            .unwrap_err(),
        HeapError::ResourceLimit {
            resource: HeapResource::ArrayLength,
            limit: 3,
            required: i32::MAX as usize
        }
    );
}

#[test]
fn aggregate_element_limit_is_independent_of_element_width_and_counts_all_arrays() {
    let mut heap = heap(HeapOptions {
        max_total_elements: 3,
        ..HeapOptions::default()
    });
    let first = heap.allocate_primitive_array(K::Byte, 2).unwrap();
    heap.store(first, 1, Value::Int(-7)).unwrap();
    assert_eq!(
        heap.allocate_primitive_array(K::Int, 2).unwrap_err(),
        HeapError::ResourceLimit {
            resource: HeapResource::TotalElements,
            limit: 3,
            required: 4
        }
    );
    assert_eq!(counters(&heap), (1, 2, 2));
    heap.allocate_primitive_array(K::Long, 1).unwrap();
    assert_eq!(counters(&heap), (2, 3, 10));
    assert_eq!(
        heap.allocate_primitive_array(K::Byte, 1).unwrap_err(),
        HeapError::ResourceLimit {
            resource: HeapResource::TotalElements,
            limit: 3,
            required: 4
        }
    );
    heap.allocate_primitive_array(K::Double, 0).unwrap();
    assert_eq!(counters(&heap), (3, 3, 10));
    assert_eq!(heap.load(first, 1).unwrap(), Value::Int(-7));
}

#[test]
fn aggregate_payload_limit_uses_actual_kind_width_and_rejected_allocations_are_atomic() {
    let mut heap = heap(HeapOptions {
        max_payload_bytes: 10,
        ..HeapOptions::default()
    });
    let first = heap.allocate_primitive_array(K::Short, 2).unwrap();
    heap.store(first, 0, Value::Int(123)).unwrap();
    heap.allocate_primitive_array(K::Int, 1).unwrap();
    assert_eq!(counters(&heap), (2, 3, 8));
    assert_eq!(
        heap.allocate_primitive_array(K::Long, 1).unwrap_err(),
        HeapError::ResourceLimit {
            resource: HeapResource::PayloadBytes,
            limit: 10,
            required: 16
        }
    );
    assert_eq!(counters(&heap), (2, 3, 8));
    heap.allocate_primitive_array(K::Byte, 2).unwrap();
    assert_eq!(counters(&heap), (3, 5, 10));
    assert_eq!(
        heap.allocate_primitive_array(K::Byte, 1).unwrap_err(),
        HeapError::ResourceLimit {
            resource: HeapResource::PayloadBytes,
            limit: 10,
            required: 11
        }
    );
    heap.allocate_primitive_array(K::Double, 0).unwrap();
    assert_eq!(counters(&heap), (4, 5, 10));
    assert_eq!(heap.load(first, 0).unwrap(), Value::Int(123));
}

#[test]
fn maximum_numeric_allowances_do_not_trigger_eager_allocation() {
    let mut heap = heap(HeapOptions {
        max_arrays: usize::MAX,
        max_array_length: usize::MAX,
        max_total_elements: usize::MAX,
        max_payload_bytes: usize::MAX,
    });
    assert_eq!(counters(&heap), (0, 0, 0));
    for kind in KINDS {
        heap.allocate_primitive_array(kind, 2).unwrap();
    }
    assert_eq!(counters(&heap), (8, 16, 60));
}
