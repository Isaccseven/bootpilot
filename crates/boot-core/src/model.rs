use crate::{BootError, BootResult};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
pub const PLAN_LIFETIME_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CollectionState {
    Complete,
    Partial,
    Unavailable,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Firmware,
    Bcd,
    BootCurrent,
    SecureBoot,
    Bitlocker,
    Grub,
    Linux,
    Volumes,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceCompleteness {
    pub firmware: CollectionState,
    pub bcd: CollectionState,
    pub boot_current: CollectionState,
    pub secure_boot: CollectionState,
    pub bitlocker: CollectionState,
    pub grub: CollectionState,
    pub linux: CollectionState,
    #[serde(default)]
    pub volumes: CollectionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceError {
    pub source: Source,
    pub error: BootError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FirmwareEntry {
    pub id: Uuid,
    pub title: String,
    pub loader_path: Option<String>,
}

/// UEFI Boot#### number, with optional BCD correlation established by collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BootCurrent {
    pub uefi_number: u16,
    pub description: Option<String>,
    pub bcd_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MenuEntryKind {
    Windows,
    Linux,
    Installer,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MenuEntry {
    /// Explicit GRUB ID, or stable parser-generated ID resolved to a menu index by the planner.
    /// Never a caller-supplied code fragment.
    pub id: String,
    pub title: String,
    pub kind: MenuEntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrubDocument {
    pub main_bytes: Vec<u8>,
    pub user_bytes: Vec<u8>,
    pub entries: Vec<MenuEntry>,
    pub default_entry_id: Option<String>,
    pub timeout_seconds: Option<u16>,
    pub write_supported: bool,
    pub write_block_reason: Option<String>,
}

/// Discovery metadata only. Write requests use closed `Target` variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrubLocation {
    pub main_path: String,
    pub user_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub source: Source,
    /// Structured snapshot field or a collector-defined evidence identifier.
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinuxCandidate {
    pub id: Uuid,
    pub title: String,
    pub volume_id: Uuid,
    pub filesystem_uuid: String,
    pub kernel_path: String,
    pub initramfs_paths: Vec<String>,
    pub kernel_parameters: Vec<String>,
    /// Set only after the collector has checked the existing volume and files.
    pub validated: bool,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub id: Uuid,
    pub captured_at: DateTime<Utc>,
    pub sources: SourceCompleteness,
    pub firmware_entries: Vec<FirmwareEntry>,
    pub firmware_order: Vec<Uuid>,
    pub boot_current: Option<BootCurrent>,
    #[serde(default)]
    pub boot_next: Option<u16>,
    #[serde(default)]
    pub loader_info: Option<String>,
    #[serde(default)]
    pub loader_entry_selected: Option<String>,
    #[serde(default)]
    pub volumes: Vec<VolumeInfo>,
    pub secure_boot: Option<bool>,
    /// Protection status of the Windows system volume; unknown is `null`.
    pub bitlocker: Option<bool>,
    pub grub: Option<GrubDocument>,
    pub grub_location: Option<GrubLocation>,
    pub linux_candidates: Vec<LinuxCandidate>,
    pub source_errors: Vec<SourceError>,
}

impl Snapshot {
    /// An empty collection is explicitly unknown, never a complete inventory.
    pub fn new(id: Uuid, captured_at: DateTime<Utc>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            captured_at,
            sources: SourceCompleteness::default(),
            firmware_entries: Vec::new(),
            firmware_order: Vec::new(),
            boot_current: None,
            boot_next: None,
            loader_info: None,
            loader_entry_selected: None,
            volumes: Vec::new(),
            secure_boot: None,
            bitlocker: None,
            grub: None,
            grub_location: None,
            linux_candidates: Vec::new(),
            source_errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeInfo {
    pub volume_id: Option<Uuid>,
    pub partition_id: Option<Uuid>,
    pub paths: Vec<String>,
    pub filesystem: Option<String>,
    pub filesystem_uuid: Option<String>,
    pub size_bytes: Option<u64>,
    pub disk_number: Option<u32>,
    pub partition_number: Option<u32>,
    pub gpt_type: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanRequest {
    Order {
        entry_ids: Vec<Uuid>,
    },
    Menu {
        default_entry_id: String,
        timeout_seconds: u16,
    },
    Cachyos {
        candidate_id: Uuid,
        title: String,
    },
    Restore {
        backup_id: Uuid,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingCertainty {
    Evidence,
    Hypothesis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub certainty: FindingCertainty,
    pub statement: String,
    pub evidence: Vec<EvidenceRef>,
    pub open_check: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    FirmwareOrder,
    GrubMain,
    GrubUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TargetValue {
    FirmwareOrder(Vec<Uuid>),
    FileBytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeDiff {
    pub target: Target,
    pub description: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Precondition {
    pub target: Target,
    /// SHA-256 over the canonical target value, computed by the planner.
    pub expected_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlannedChange {
    pub target: Target,
    pub expected: TargetValue,
    pub replacement: TargetValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    Ready,
    Started,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangePlan {
    pub schema_version: u32,
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub snapshot_id: Uuid,
    pub request: PlanRequest,
    pub diffs: Vec<ChangeDiff>,
    pub preconditions: Vec<Precondition>,
    pub changes: Vec<PlannedChange>,
    pub state: PlanState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepPhase {
    Intent,
    Written,
    Verified,
    RollbackIntent,
    RolledBack,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JournalStep {
    pub sequence: u32,
    pub target: Target,
    pub phase: StepPhase,
    pub at: DateTime<Utc>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplyOutcome {
    Applied,
    RolledBack,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Deviation {
    pub target: Target,
    pub description: String,
    pub expected: Option<TargetValue>,
    pub actual: Option<TargetValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyReport {
    pub schema_version: u32,
    pub plan_id: Uuid,
    pub backup_id: Option<Uuid>,
    pub outcome: ApplyOutcome,
    pub verified_steps: Vec<JournalStep>,
    pub remaining_deviations: Vec<Deviation>,
    /// Remains false unless a separately authorized real boot was observed.
    pub boot_tested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BackupTarget {
    pub target: Target,
    pub original: TargetValue,
    pub applied: TargetValue,
    pub original_checksum: String,
    pub applied_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BackupRecord {
    pub schema_version: u32,
    pub id: Uuid,
    pub plan_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub request: PlanRequest,
    pub targets: Vec<BackupTarget>,
    pub journal: Vec<JournalStep>,
    /// None denotes an unfinished journal and must remain visible on restart.
    pub outcome: Option<ApplyReport>,
}

pub trait SystemAccess {
    fn read_snapshot(&mut self) -> BootResult<Snapshot>;
    fn read_target(&mut self, target: &Target) -> BootResult<TargetValue>;
    /// Recheck expected immediately before mutation; verify non-atomic firmware writes afterwards.
    fn write_target(
        &mut self,
        target: &Target,
        expected: &TargetValue,
        replacement: &TargetValue,
    ) -> BootResult<()>;
}

pub trait PlanStore {
    fn save_plan(&mut self, plan: &ChangePlan) -> BootResult<()>;
    fn load_plan(&self, id: Uuid) -> BootResult<ChangePlan>;
    /// Durably starts/consumes the plan before any target write. Must reject an already started plan.
    fn begin_backup(
        &mut self,
        plan: &ChangePlan,
        originals: &[(Target, TargetValue)],
    ) -> BootResult<BackupRecord>;
    fn record_step(&mut self, id: Uuid, step: &JournalStep) -> BootResult<()>;
    fn finish_plan(&mut self, id: Uuid, outcome: &ApplyReport) -> BootResult<()>;
    /// Includes incomplete journals; listing is never limited to successful changes.
    fn list_backups(&self) -> BootResult<Vec<BackupRecord>>;
    fn load_backup(&self, id: Uuid) -> BootResult<BackupRecord>;
}
