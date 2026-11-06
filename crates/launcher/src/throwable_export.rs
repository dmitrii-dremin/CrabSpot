//! Bounded, copied head-only diagnostics before the image releases its heap.
use crate::{
    ApplicationCompletion, ApplicationErrorKind as K, ApplicationExceptionLocation,
    ApplicationResource as R, ApplicationStage as S, ApplicationUsage, EntryEvaluation, SetupKind,
    error::Result, meter::Meter,
};
use crabspot_classfile::JavaString;
use crabspot_interpreter::InitializationSession;
use crabspot_runtime::Reference;
fn copy(units: &[u16], meter: &Meter, usage: ApplicationUsage) -> Result<JavaString> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(units.len())
        .map_err(|_| meter.alloc(S::Setup, usage))?;
    result.extend_from_slice(units);
    Ok(JavaString::from_units(result))
}
pub(crate) fn uncaught(
    session: &mut InitializationSession<'_, '_>,
    reference: Reference,
    phase: EntryEvaluation,
    meter: &mut Meter,
    usage: ApplicationUsage,
) -> Result<ApplicationCompletion> {
    meter.work(1, S::Setup, usage)?;
    let class = session
        .context()
        .heap()
        .instance_class(reference)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::Instance,
                    source: Box::new(source),
                },
                usage,
            )
        })?;
    meter.work(1, S::Setup, usage)?;
    let metadata = session
        .context()
        .registry()
        .class(class)
        .map_err(|e| meter.error(S::Setup, K::Registry(e), usage))?;
    meter.work(2, S::Setup, usage)?;
    let name = metadata
        .name()
        .map_err(|e| meter.error(S::Setup, K::Parse(e), usage))?;
    meter.charge(
        name.units().len() as u64,
        name.units().len() as u64 + 1,
        1,
        S::Setup,
        usage,
    )?;
    let class_name = copy(name.units(), meter, usage)?;
    let head = {
        let trace = session.throwable_trace(reference).map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::Throwable,
                    source: Box::new(source),
                },
                usage,
            )
        })?;
        if let Some(head) = trace.and_then(|trace| trace.first()) {
            meter.retain(1, S::Setup, usage)?;
            Some(*head)
        } else {
            None
        }
    };
    let location = if let Some(head) = head {
        meter.work(2, S::Setup, usage)?;
        let owner = session
            .context()
            .registry()
            .class(head.method.owner())
            .map_err(|e| meter.error(S::Setup, K::Registry(e), usage))?;
        let method = owner.methods.get(head.method.index()).ok_or_else(|| {
            meter.error(S::Setup, K::Invariant("trace method index differs"), usage)
        })?;
        meter.work(2, S::Setup, usage)?;
        let class = owner
            .name()
            .map_err(|e| meter.error(S::Setup, K::Parse(e), usage))?;
        meter.work(1, S::Setup, usage)?;
        let name = owner
            .constant_pool
            .utf8(method.name_index)
            .map_err(|e| meter.error(S::Setup, K::Parse(e), usage))?;
        meter.work(1, S::Setup, usage)?;
        let descriptor = owner
            .constant_pool
            .utf8(method.descriptor_index)
            .map_err(|e| meter.error(S::Setup, K::Parse(e), usage))?;
        let units = class
            .units()
            .len()
            .checked_add(name.units().len())
            .and_then(|n| n.checked_add(descriptor.units().len()))
            .ok_or_else(|| {
                meter.error(
                    S::Setup,
                    K::AccountingOverflow {
                        resource: R::Utf16Units,
                    },
                    usage,
                )
            })? as u64;
        let work = units.checked_add(3).ok_or_else(|| {
            meter.error(
                S::Setup,
                K::AccountingOverflow {
                    resource: R::WorkUnits,
                },
                usage,
            )
        })?;
        meter.charge(units, work, 3, S::Setup, usage)?;
        Some(ApplicationExceptionLocation {
            class_name: copy(class.units(), meter, usage)?,
            method_name: copy(name.units(), meter, usage)?,
            descriptor: copy(descriptor.units(), meter, usage)?,
            offset: head.pc,
        })
    } else {
        None
    };
    Ok(ApplicationCompletion::Uncaught {
        phase,
        class_name,
        location,
    })
}

#[cfg(test)]
#[path = "throwable_export_tests.rs"]
mod tests;
