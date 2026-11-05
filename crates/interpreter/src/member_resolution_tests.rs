use super::*;
use crabspot_runtime::{MemberResolutionError, MemberResolutionErrorKind};
#[test]
fn direct_linkage_variants_map_to_existing_symbolic_and_vm_routes() {
    assert!(matches!(
        field_failure(FieldAccessError::Java(FieldJavaError::LinkageError), 7).unwrap(),
        Action::LinkageError
    ));
    assert!(matches!(
        static_failure(StaticAccessError::Java(StaticJavaError::LinkageError), 7).unwrap(),
        Action::LinkageError
    ));
    assert_eq!(
        method_failure(MethodAccessError::Java(MethodJavaError::LinkageError), 7).unwrap(),
        ExecutionOutcome::LinkageError
    );
    assert_eq!(
        vm_kind(ExecutionOutcome::LinkageError),
        Some(VmExceptionKind::LinkageError)
    );
    let e = vm_completion(ExecutionOutcome::LinkageError, true, 7)
        .err()
        .unwrap();
    assert_eq!(e.offset, Some(7));
    assert_eq!(
        e.kind,
        E::UnsupportedVmException(VmExceptionKind::LinkageError)
    );
    let e = construction_failure(
        ConstructionAccessError::Java(ConstructionJavaError::LinkageError),
        7,
    );
    assert_eq!(
        e.kind,
        E::UnsupportedVmException(VmExceptionKind::LinkageError)
    );
    let e = initialization_failure(
        InitializationAccessError::Java(StaticJavaError::LinkageError),
        Some(7),
    );
    assert_eq!(
        e.kind,
        E::UnsupportedVmException(VmExceptionKind::LinkageError)
    );
}
#[test]
fn nested_java_shaped_resolution_sources_are_host_errors_without_projection() {
    let nested = FieldAccessError::MemberResolution(Box::new(MemberResolutionError {
        kind: MemberResolutionErrorKind::Field(Box::new(FieldAccessError::Java(
            FieldJavaError::LinkageError,
        ))),
        origin: None,
        index: None,
        position: None,
    }));
    let e = field_failure(nested.clone(), 9).err().unwrap();
    assert_eq!(e.kind, E::FieldAccess(Box::new(nested.clone())));
    assert_eq!(e.offset, Some(9));
    let method = MethodAccessError::Context(nested.clone());
    let e = method_failure(method.clone(), 9).unwrap_err();
    assert_eq!(e.kind, E::MethodAccess(Box::new(method)));
    let statics = StaticAccessError::Context(nested.clone());
    let e = static_failure(statics.clone(), 9).err().unwrap();
    assert_eq!(e.kind, E::StaticAccess(Box::new(statics)));
    let c = ConstructionAccessError::Field(Box::new(nested));
    let e = construction_failure(c.clone(), 9);
    assert_eq!(e.kind, E::ConstructionAccess(Box::new(c)));
}
