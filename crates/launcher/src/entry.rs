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
    invocation: crate::ApplicationInvocation<'_>,
) -> Result<ApplicationRun> {
    let crate::ApplicationInvocation {
        arguments: input_arguments,
        monitors,
        mirrors,
        strings,
    } = invocation;
    let PreparedImage {
        mut domain,
        mut heap,
        string_body_plan,
        string_body_limits,
        string_equality_plan,
        bootstrap_assertions,
        throwable_plan,
        throwable_profile,
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
    domain
        .enable_class_mirror_sites(mirrors.max_class_sites)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::ClassMirror,
                    source,
                },
                usage,
            )
        })?;
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
    heap.bind_class_mirrors(domain.registry(), roles.core.class, mirrors.max_mirrors)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::ClassMirror,
                    source,
                },
                usage,
            )
        })?;
    heap.bind_strings(domain.registry(), roles.core.string, strings.strings)
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: SetupKind::String,
                    source,
                },
                usage,
            )
        })?;
    // Capture every post-binding exit, including setup failures before an evaluation.
    let result = (|| {
        if let Some(plan) = &string_body_plan {
            heap.bind_compact_string_bodies(domain.registry(), plan, string_body_limits)
                .map_err(|source| {
                    meter.error(
                        S::Setup,
                        K::Setup {
                            kind: SetupKind::StringBody,
                            source,
                        },
                        usage,
                    )
                })?;
        }
        if let Some(plan) = &string_equality_plan {
            heap.bind_compact_string_equality(domain.registry(), plan, bootstrap_assertions)
                .map_err(|source| {
                    meter.error(
                        S::Setup,
                        K::Setup {
                            kind: SetupKind::StringBody,
                            source,
                        },
                        usage,
                    )
                })?;
        }
        if let Some(plan) = &throwable_plan {
            heap.bind_throwable_support(domain.registry(), plan, throwable_profile.traces)
                .map_err(|source| meter.error(S::Layout, K::Throwable(source), usage))?;
        }
        let mut context = domain
            .member_context_unnamed_modules(&mut heap, options.field, options.loading_work)
            .map_err(|source| meter.error(S::Setup, K::Member(source), usage))?;
        let main = context
            .resolve_application_main_with_monitors(named)
            .map_err(|source| {
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
            .allocate_reference_array(
                ReferenceType::Class(roles.core.string),
                i32::try_from(input_arguments.len()).map_err(|_| {
                    meter.error(
                        S::Input,
                        K::Invariant("validated argument count exceeded Java int"),
                        usage,
                    )
                })?,
            )
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
        for (index, units) in input_arguments.iter().enumerate() {
            let value = context.fresh_string(units).map_err(|source| {
                meter.error(
                    S::Setup,
                    K::Setup {
                        kind: SetupKind::String,
                        source,
                    },
                    usage,
                )
            })?;
            // The prevalidated count is at most i32::MAX, so both counters are funded.
            usage.argument_strings += 1;
            context
                .write_reference_array(arguments, index as i32, value)
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
        }
        let mut session = if throwable_plan.is_some() {
            InitializationSession::new_with_throwables(
                &mut context,
                &statics,
                &exceptions,
                roles.error,
                options.initialization,
                monitors,
                throwable_profile.vm_exceptions,
            )
        } else {
            InitializationSession::new_with_strings(
                &mut context,
                &statics,
                &exceptions,
                roles.error,
                options.initialization,
                monitors,
            )
        }
        .map_err(|source| {
            meter.error(
                S::Setup,
                K::Setup {
                    kind: if throwable_plan.is_some() {
                        SetupKind::Throwable
                    } else {
                        SetupKind::Initialization
                    },
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
        // Preserve private session history even if later status/observation fails.
        usage.string_equality = session.string_equality_usage();
        usage.vm_exceptions = session.vm_exception_usage();
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
                let phase = failure_phase(status, error.initialization_class.is_some())
                    .ok_or_else(|| {
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
                            K::Invariant(
                                "uncaught entry has an inconsistent initialization record",
                            ),
                            usage,
                        ));
                    }
                };
                if throwable_plan.is_some() {
                    crate::throwable_export::uncaught(
                        &mut session,
                        reference,
                        phase,
                        &mut meter,
                        usage,
                    )?
                } else {
                    let class =
                        session
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
    })();
    let trace_usage = heap.throwable_trace_usage();
    let string_usage = heap.string_usage();
    let string_body_usage = heap.string_body_usage();
    match result {
        Ok(mut run) => {
            run.usage.traces = trace_usage;
            run.usage.strings = string_usage;
            run.usage.string_bodies = string_body_usage;
            Ok(run)
        }
        Err(mut error) => {
            error.usage.traces = trace_usage;
            error.usage.strings = string_usage;
            error.usage.string_bodies = string_body_usage;
            Err(error)
        }
    }
}
