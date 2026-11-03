//! Evaluate one genuine compiled method body with an explicit managed heap.
//! This example does not perform class initialization or Java application startup.
use crabspot_classfile::parse;
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, Value, execute_static_method_body_with_heap,
};
use crabspot_runtime::{HeapOptions, ManagedHeap};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let length: i32 = match arguments.next() {
        Some(argument) => argument.parse()?,
        None => 17,
    };
    if arguments.next().is_some() {
        return Err("usage: primitive_array_body [length]".into());
    }
    let class = parse(include_bytes!(
        "../../../tests/fixtures/classes/fixtures/ArrayCases.class"
    ))?;
    let method = class
        .methods
        .iter()
        .position(|method| {
            class
                .constant_pool
                .utf8(method.name_index)
                .is_ok_and(|name| name.equals_ascii("filledSum"))
                && class
                    .constant_pool
                    .utf8(method.descriptor_index)
                    .is_ok_and(|descriptor| descriptor.equals_ascii("(I)I"))
        })
        .ok_or("fixture does not declare filledSum(I)I")?;
    let mut heap = ManagedHeap::new(HeapOptions::default())?;
    let result = execute_static_method_body_with_heap(
        &class,
        method,
        &[Value::Int(length)],
        ExecutionOptions::default(),
        CallLimits::default(),
        &mut heap,
    );
    println!("CrabSpot method-body demo: filledSum({length})");
    println!(
        "Heap: {} array(s), {} elements, {} payload bytes",
        heap.allocated_arrays(),
        heap.total_elements(),
        heap.payload_bytes()
    );
    let outcome = result?;
    println!("Outcome: {outcome:?}");
    Ok(())
}
