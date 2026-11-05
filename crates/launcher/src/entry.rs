//! The only outer execution adapter for a proved, prepared, sealed image.
use crate::{
    ApplicationCompletion, ApplicationErrorKind as K, ApplicationRun, ApplicationStage as S,
    EntryEvaluation, SetupKind, StaticIntObservation,
    error::Result,
    image::{PreparedImage, resolve_observations},
};
use crabspot_interpreter::{ExceptionOutcome, InitializationSession, InitializationStatus};
use crabspot_runtime::{ReferenceType, ReferenceTypeUniverse, Value};

fn failure_phase(status: InitializationStatus, has_initializer: bool) -> Option<EntryEvaluation> {
    match status {
        InitializationStatus::Completed => Some(EntryEvaluation::MainEvaluation),
        InitializationStatus::Uninitialized if !has_initializer => {
            Some(EntryEvaluation::RootPreflight)
        }
        InitializationStatus::Uninitialized
        | InitializationStatus::Erroneous { .. }
        | InitializationStatus::Aborted => Some(EntryEvaluation::NamedInitialization),
        InitializationStatus::InProgress => None,
    }
}

pub(crate) fn run_prepared(
    image: PreparedImage,
    observations: &[StaticIntObservation<'_>],
) -> Result<ApplicationRun> {
    let PreparedImage {
        mut domain,
        mut heap,
        roles,
        named,
        app_loader,
        routes,
        options,
        mut usage,
        mut meter,
    } = image;
    if usage.argument_arrays != 0 || usage.engine_evaluations != 0 {
        return Err(meter.error(
            S::Setup,
            K::Invariant("image was already dispatched"),
            usage,
        ));
    }
    heap.bind_reference_types(
        domain.registry(),
        ReferenceTypeUniverse {
            bootstrap_loader: roles.core.bootstrap_loader,
            object: roles.core.object,
            cloneable: roles.core.cloneable,
            serializable: roles.core.serializable,
        },
    )
    .map_err(|source| {
        meter.error(
            S::Setup,
            K::Setup {
                kind: SetupKind::Type,
                source: Box::new(source),
            },
            usage,
        )
    })?;
    let mut context = domain
        .member_context_unnamed_modules(&mut heap, options.field, options.loading_work)
        .map_err(|source| meter.error(S::Setup, K::Member(source), usage))?;
    let main = context.resolve_application_main(named).map_err(|source| {
        meter.error(
            S::Entry,
            K::Setup {
                kind: SetupKind::Main,
                source,
            },
            usage,
        )
    })?;
    let fields = resolve_observations(
        &routes,
        context.registry(),
        app_loader,
        observations,
        &mut meter,
        usage,
    )?;
    // Reserve every requested result before the engine starts. A late output-vector
    // policy/allocation failure must not erase the actual execution completion.
    meter.retain(fields.len(), S::Setup, usage)?;
    let mut static_ints = Vec::new();
    static_ints
        .try_reserve_exact(fields.len())
        .map_err(|_| meter.alloc(S::Setup, usage))?;
    static_ints.resize(fields.len(), 0);
    let statics = context
        .allocate_static_state(options.statics)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::Static,
                    source: Box::new(source),
                },
                usage,
            )
        })?;
    let exceptions = context
        .prepare_exception_profile(roles.core.throwable)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::Exception,
                    source: Box::new(source),
                },
                usage,
            )
        })?;
    let arguments = context
        .allocate_reference_array(ReferenceType::Class(roles.core.string), 0)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::Array,
                    source: Box::new(source),
                },
                usage,
            )
        })?;
    usage.argument_arrays = 1;
    let mut session = InitializationSession::new(
        &mut context,
        &statics,
        &exceptions,
        roles.error,
        options.initialization,
    )
    .map_err(|source| {
        meter.error(
            S::Setup,
            K::Setup {
                kind: SetupKind::Initialization,
                source: Box::new(source),
            },
            usage,
        )
    })?;
    usage.engine_evaluations = 1;
    let outcome = session.run_application_main(
        &main,
        arguments,
        options.execution,
        options.calls,
        options.exceptions,
        options.construction,
    );
    let status = session.status(named).map_err(|source| {
        meter.error(
            S::Setup,
            K::Setup {
                kind: SetupKind::Initialization,
                source: Box::new(source),
            },
            usage,
        )
    })?;
    let completion = match outcome {
        Ok(ExceptionOutcome::Returned(None)) if status == InitializationStatus::Completed => {
            ApplicationCompletion::Returned
        }
        Ok(ExceptionOutcome::Returned(_)) => {
            return Err(meter.error(
                S::Setup,
                K::Invariant("main returned a value or its named class did not complete"),
                usage,
            ));
        }
        Err(error) => {
            let phase =
                failure_phase(status, error.initialization_class.is_some()).ok_or_else(|| {
                    meter.error(
                        S::Setup,
                        K::Invariant("initialization left an active entry record"),
                        usage,
                    )
                })?;
            ApplicationCompletion::ExecutionFailed {
                phase,
                error: Box::new(error),
            }
        }
        Ok(ExceptionOutcome::Uncaught(reference)) => {
            let phase = match status {
                InitializationStatus::Completed => EntryEvaluation::MainEvaluation,
                InitializationStatus::Erroneous { cause } if cause == reference => {
                    EntryEvaluation::NamedInitialization
                }
                _ => {
                    return Err(meter.error(
                        S::Setup,
                        K::Invariant("uncaught entry has an inconsistent initialization record"),
                        usage,
                    ));
                }
            };
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
            let metadata = session
                .context()
                .registry()
                .class(class)
                .map_err(|source| meter.error(S::Setup, K::Registry(source), usage))?;
            // An owned bounded diagnostic name follows the contract's normal diagnostic
            // allocation convention; no heap reference escapes the one-shot image.
            let class_name = metadata
                .name()
                .map_err(|source| meter.error(S::Setup, K::Parse(source), usage))?
                .clone();
            ApplicationCompletion::SuppliedUncaught { phase, class_name }
        }
    };
    for (slot, field) in static_ints.iter_mut().zip(fields) {
        match session
            .context()
            .inspect_static(&statics, field)
            .map_err(|source| {
                meter.error(
                    S::Setup,
                    K::Setup {
                        kind: SetupKind::Static,
                        source: Box::new(source),
                    },
                    usage,
                )
            })? {
            Value::Int(n) => *slot = n,
            _ => {
                return Err(meter.error(
                    S::Setup,
                    K::Invariant("observed static I slot has another computational kind"),
                    usage,
                ));
            }
        }
    }
    Ok(ApplicationRun {
        completion,
        static_ints,
        usage: meter.snapshot(usage),
    })
}
