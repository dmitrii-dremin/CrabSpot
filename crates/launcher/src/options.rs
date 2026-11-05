use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ConstructionLimits, ExceptionLimits, ExecutionOptions, InitializationLimits,
};
use crabspot_runtime::{
    FieldResolutionOptions, HeapOptions, HierarchyOptions, InstanceOptions, LoadingDomainLimits,
    LoadingWorkLimits, MemberResolutionLimits, RegistryOptions, StaticStateOptions,
};
use crabspot_verifier::{
    ClassTypeVerificationLimits, ClassTypeVerificationOptions, VerificationEnvironmentOptions,
};
#[derive(Clone, Copy, Debug)]
pub struct ImageLimits {
    pub max_definitions: u64,
    pub max_input_bytes: u64,
    pub max_file_bytes: u64,
    pub max_entries: u64,
    pub max_depth: u64,
    pub max_path_units: u64,
    pub max_total_path_units: u64,
    pub max_observations: u64,
    pub max_inventory_work: u64,
    pub max_scratch_entries: u64,
    pub max_utf16_units: u64,
}
impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_definitions: 16_384,
            max_input_bytes: 256 << 20,
            max_file_bytes: 16 << 20,
            max_entries: 65_536,
            max_depth: 128,
            max_path_units: 4_096,
            max_total_path_units: 16_777_216,
            max_observations: 256,
            max_inventory_work: 1_073_741_824,
            max_scratch_entries: 4_194_304,
            max_utf16_units: 134_217_728,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ApplicationOptions {
    pub image: ImageLimits,
    pub parse: ParseOptions,
    pub registry: RegistryOptions,
    pub hierarchy: HierarchyOptions,
    pub environment: VerificationEnvironmentOptions,
    pub class: ClassTypeVerificationOptions,
    pub verification_total: ClassTypeVerificationLimits,
    pub domain: LoadingDomainLimits,
    pub member: MemberResolutionLimits,
    pub loading_work: LoadingWorkLimits,
    pub field: FieldResolutionOptions,
    pub heap: HeapOptions,
    pub instance: InstanceOptions,
    pub statics: StaticStateOptions,
    pub initialization: InitializationLimits,
    pub execution: ExecutionOptions,
    pub calls: CallLimits,
    pub exceptions: ExceptionLimits,
    pub construction: ConstructionLimits,
}
impl Default for ApplicationOptions {
    fn default() -> Self {
        Self {
            image: Default::default(),
            parse: Default::default(),
            registry: RegistryOptions {
                max_loaders: 2,
                max_definitions: 16_384,
                max_definition_bytes: 16 << 20,
                max_total_definition_bytes: 256 << 20,
            },
            hierarchy: Default::default(),
            environment: Default::default(),
            class: Default::default(),
            verification_total: ClassTypeVerificationLimits {
                max_methods: 4_194_304,
                max_code_bytes: 268_435_456,
                max_code_metadata_entries: 268_435_456,
                max_code_utf16_units: 1_073_741_824,
                max_code_work_units: 2_147_483_648,
                max_type_symbols: 67_108_864,
                max_state_cells: 1_073_741_824,
                max_edge_checks: 1_073_741_824,
                max_type_utf16_units: 2_147_483_648,
                max_type_work_units: 4_294_967_296,
            },
            domain: Default::default(),
            member: Default::default(),
            loading_work: Default::default(),
            field: Default::default(),
            heap: Default::default(),
            instance: Default::default(),
            statics: Default::default(),
            initialization: Default::default(),
            execution: Default::default(),
            calls: Default::default(),
            exceptions: Default::default(),
            construction: Default::default(),
        }
    }
}
