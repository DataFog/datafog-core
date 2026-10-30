"""Types for the compiled API. Underscored helpers exist only for type checking."""

from collections.abc import Awaitable, Iterable, Sequence
from typing import Literal, NoReturn, Protocol, TypeAlias, TypedDict, final

_JsonValue: TypeAlias = (
    None | bool | int | float | str | list["_JsonValue"] | dict[str, "_JsonValue"]
)
_JsonDocument: TypeAlias = list[_JsonValue] | dict[str, _JsonValue]

class _ModelLimits(TypedDict):
    max_input_bytes: int
    max_response_bytes: int
    max_findings: int
    timeout_ms: int
    cleanup_timeout_ms: int

class _ModelConfigRequired(TypedDict):
    bundle: str
    manifest_sha256: str
    limits: _ModelLimits

class _Gliner2LabelRequired(TypedDict):
    label: str
    threshold: float

class _Gliner2Label(_Gliner2LabelRequired, total=False):
    description: str

class _Gliner2Config(TypedDict):
    python: str
    labels: list[_Gliner2Label]
    intra_threads: int
    chunk_size: int
    chunk_overlap: int

class _ModelConfig(_ModelConfigRequired, total=False):
    engine: Literal["datafog", "gliner2"]
    gliner2: _Gliner2Config

class _ModelDescriptor(TypedDict):
    backend: str
    model: str
    adapter_version: str
    labels: dict[str, str]
    score_scale: str

class DataFogModelError(RuntimeError):
    code: str
    reason: str
    path: str
    finding_index: int | None

@final
class ModelScanResult:
    @property
    def contract_version(self) -> int: ...
    @property
    def model(self) -> str: ...
    @property
    def findings(self) -> list[Finding]: ...
    @property
    def evidence(self) -> list[dict[str, object]]: ...

@final
class ModelCancellation:
    def __init__(self) -> None: ...
    def cancel(self) -> None: ...

@final
class ModelSession:
    @staticmethod
    def open(config: _ModelConfig) -> ModelSession: ...
    def describe(self) -> _ModelDescriptor: ...
    def scan(self, text: str, config: _ScanConfig | None = None, *, cancellation: ModelCancellation | None = None) -> ModelScanResult: ...
    def scan_model(self, text: str, *, cancellation: ModelCancellation | None = None) -> ModelScanResult: ...
    def close(self) -> None: ...
    def __enter__(self) -> ModelSession: ...
    def __exit__(self, kind: object, value: object, traceback: object) -> bool: ...

class _CommonScanConfig(TypedDict, total=False):
    format: Literal["text", "env", "sql"]
    locale: str
    detect_uuid: bool

class _ScanConfig(_CommonScanConfig, total=False):
    # Select activated text detectors; omission preserves the default scan.
    entities: list[str] | tuple[str, ...]

class _CapabilityActivationRequired(TypedDict):
    kind: Literal["default", "locale", "config", "structured"]

class _CapabilityActivation(_CapabilityActivationRequired, total=False):
    scan_config: _ScanConfig

class _EntityCapability(TypedDict):
    scopes: list[Literal["structured", "text"]]
    activation: _CapabilityActivation

class _LocaleCapability(TypedDict):
    enabled_entities: list[str]

class _Capabilities(TypedDict):
    contract_version: Literal[1]
    supported_entities: list[str]
    default_entities: list[str]
    locales: dict[str, _LocaleCapability]
    entities: dict[str, _EntityCapability]

class _StructuredScanConfig(_CommonScanConfig, total=False):
    discover_person: bool
    mappings: dict[str, Literal["PERSON"]]
    exclude: list[str] | tuple[str, ...]

class _Reveal(TypedDict):
    direction: Literal["first", "last"]
    count: int

class _Redact(TypedDict):
    strategy: Literal["redact"]

class _Remove(TypedDict):
    strategy: Literal["remove"]

class _MaskRequired(TypedDict):
    strategy: Literal["mask"]

class _Mask(_MaskRequired, total=False):
    character: str
    reveal: _Reveal

class _PseudonymizeRequired(TypedDict):
    strategy: Literal["pseudonymize"]
    key_ref: str

class _Pseudonymize(_PseudonymizeRequired, total=False):
    key_version: str

class _Tokenize(TypedDict):
    strategy: Literal["tokenize"]
    token_ref: str

_StrategyConfig: TypeAlias = _Redact | _Remove | _Mask | _Pseudonymize | _Tokenize

class _RegexRequired(TypedDict):
    pattern: str

class _Regex(_RegexRequired, total=False):
    case_sensitive: bool

class _Allow(TypedDict, total=False):
    exact: dict[str, list[str] | tuple[str, ...]]
    regex: dict[str, list[_Regex] | tuple[_Regex, ...]]

class _TransformRequired(TypedDict):
    default: _StrategyConfig

class _TransformationConfig(_TransformRequired, total=False):
    entities: list[str] | tuple[str, ...]
    overrides: dict[str, _StrategyConfig]
    allow: _Allow

class _ScanAndTransformRequired(TypedDict):
    transform: _TransformationConfig

class _ScanAndTransformConfig(_ScanAndTransformRequired, total=False):
    scan: _ScanConfig

class _StructuredScanAndTransformConfig(_ScanAndTransformRequired, total=False):
    scan: _StructuredScanConfig

class _PrivacyContext(TypedDict):
    scope: str

class _ResolvedKey(TypedDict):
    key: bytes | Sequence[int]
    resolved_version: str

class _ResolvedKeyObject(Protocol):
    @property
    def key(self) -> bytes | Sequence[int]: ...
    @property
    def resolved_version(self) -> str: ...

class _KeyProvider(Protocol):
    def resolve_key(
        self, key_ref: str, key_version: str | None, /
    ) -> Awaitable[_ResolvedKey | _ResolvedKeyObject]: ...

class _TokenizeItem(TypedDict):
    id: str
    exact_value: str
    token_ref: str

class _TokenizeResult(TypedDict):
    id: str
    payload: bytes | Sequence[int]
    resolved_version: str

class _TokenizeResultObject(Protocol):
    @property
    def id(self) -> str: ...
    @property
    def payload(self) -> bytes | Sequence[int]: ...
    @property
    def resolved_version(self) -> str: ...

class _RestoreItem(TypedDict):
    id: str
    token_ref: str
    resolved_version: str
    payload: bytes

class _RestoreResult(TypedDict):
    id: str
    value: str

class _RestoreResultObject(Protocol):
    @property
    def id(self) -> str: ...
    @property
    def value(self) -> str: ...

class _TokenProvider(Protocol):
    def tokenize_batch(
        self, scope: str, items: list[_TokenizeItem], /
    ) -> Awaitable[Iterable[_TokenizeResult | _TokenizeResultObject]]: ...
    def restore_batch(
        self, scope: str, items: list[_RestoreItem], /
    ) -> Awaitable[Iterable[_RestoreResult | _RestoreResultObject]]: ...

class DataFogConfigurationError(ValueError):
    code: str
    reason: str | None
    path: str | None
    finding_index: int | None

class DataFogFindingError(ValueError):
    code: str
    reason: str | None
    path: str | None
    finding_index: int | None

class DataFogInternalError(RuntimeError):
    code: str
    reason: str | None
    path: str | None
    finding_index: int | None

class DataFogKeyProviderError(RuntimeError):
    code: str
    reason: str | None
    path: str | None
    finding_index: int | None

@final
class TextRange:
    def __init__(self, start: int, end: int) -> None: ...
    @property
    def start(self) -> int: ...
    @property
    def end(self) -> int: ...

@final
class Finding:
    def __init__(
        self,
        entity_type: str,
        matched_text: str,
        byte_range: TextRange,
        codepoint_range: TextRange,
        detector_name: str,
        confidence: float | None = None,
        detector_version: str | None = None,
    ) -> None: ...
    @property
    def entity_type(self) -> str: ...
    @property
    def matched_text(self) -> str: ...
    @property
    def byte_range(self) -> TextRange: ...
    @property
    def codepoint_range(self) -> TextRange: ...
    @property
    def confidence(self) -> float | None: ...
    @property
    def detector_name(self) -> str: ...
    @property
    def detector_version(self) -> str | None: ...

# NoReturn prevents construction of native result-only classes on Python 3.10
# without requiring typing_extensions.Never.
@final
class Transformation:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def entity_type(self) -> str: ...
    @property
    def source_byte_range(self) -> TextRange: ...
    @property
    def source_codepoint_range(self) -> TextRange: ...
    @property
    def confidence(self) -> float | None: ...
    @property
    def detector_name(self) -> str: ...
    @property
    def detector_version(self) -> str | None: ...
    @property
    def strategy(self) -> str: ...
    @property
    def replacement(self) -> str: ...
    @property
    def output_byte_range(self) -> TextRange: ...
    @property
    def output_codepoint_range(self) -> TextRange: ...
    @property
    def key_ref(self) -> str | None: ...
    @property
    def resolved_key_version(self) -> str | None: ...
    @property
    def token_ref(self) -> str | None: ...
    @property
    def resolved_token_version(self) -> str | None: ...

@final
class TransformResult:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def text(self) -> str: ...
    @property
    def transformations(self) -> list[Transformation]: ...

@final
class Restoration:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def source_byte_range(self) -> TextRange: ...
    @property
    def source_codepoint_range(self) -> TextRange: ...
    @property
    def output_byte_range(self) -> TextRange: ...
    @property
    def output_codepoint_range(self) -> TextRange: ...
    @property
    def token_ref(self) -> str: ...
    @property
    def resolved_token_version(self) -> str: ...

@final
class RestoreResult:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def text(self) -> str: ...
    @property
    def restorations(self) -> list[Restoration]: ...

@final
class FieldMapping:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def path(self) -> str: ...
    @property
    def entity_type(self) -> str: ...
    @property
    def source(self) -> str: ...
    @property
    def rule(self) -> str: ...

@final
class StructuredFinding:
    def __init__(self, path: str, finding: Finding) -> None: ...
    @property
    def path(self) -> str: ...
    @property
    def finding(self) -> Finding: ...

@final
class StructuredScanResult:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def mappings(self) -> list[FieldMapping]: ...
    @property
    def findings(self) -> list[StructuredFinding]: ...

@final
class StructuredTransformation:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def path(self) -> str: ...
    @property
    def transformation(self) -> Transformation: ...

@final
class StructuredTransformResult:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def data(self) -> _JsonDocument: ...
    @property
    def transformations(self) -> list[StructuredTransformation]: ...

@final
class StructuredRestoration:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def path(self) -> str: ...
    @property
    def restoration(self) -> Restoration: ...

@final
class StructuredRestoreResult:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @property
    def data(self) -> _JsonDocument: ...
    @property
    def restorations(self) -> list[StructuredRestoration]: ...

@final
class PrivacyManager:
    def __init__(
        self,
        provider: _KeyProvider | None = None,
        token_provider: _TokenProvider | None = None,
    ) -> None: ...
    def transform(
        self,
        text: str,
        findings: Sequence[Finding],
        config: _TransformationConfig,
        context: _PrivacyContext | None = None,
    ) -> Awaitable[TransformResult]: ...
    def scan_and_transform(
        self,
        text: str,
        config: _ScanAndTransformConfig,
        context: _PrivacyContext | None = None,
    ) -> Awaitable[TransformResult]: ...
    def restore(
        self, text: str, context: _PrivacyContext
    ) -> Awaitable[RestoreResult]: ...
    def transform_structured(
        self,
        data: _JsonDocument,
        findings: Sequence[StructuredFinding],
        config: _TransformationConfig,
        context: _PrivacyContext | None = None,
    ) -> Awaitable[StructuredTransformResult]: ...
    def scan_and_transform_structured(
        self,
        data: _JsonDocument,
        config: _StructuredScanAndTransformConfig,
        context: _PrivacyContext | None = None,
    ) -> Awaitable[StructuredTransformResult]: ...
    def restore_structured(
        self, data: _JsonDocument, context: _PrivacyContext
    ) -> Awaitable[StructuredRestoreResult]: ...

def capabilities() -> _Capabilities: ...

def scan(text: str, config: _ScanConfig | None = None) -> list[Finding]: ...
def transform(
    text: str, findings: Sequence[Finding], config: _TransformationConfig
) -> TransformResult: ...
def scan_and_transform(
    text: str, config: _ScanAndTransformConfig
) -> TransformResult: ...
def discover_fields(
    data: _JsonDocument, config: _StructuredScanConfig | None = None
) -> list[FieldMapping]: ...
def scan_structured(
    data: _JsonDocument, config: _StructuredScanConfig | None = None
) -> StructuredScanResult: ...
def transform_structured(
    data: _JsonDocument,
    findings: Sequence[StructuredFinding],
    config: _TransformationConfig,
) -> StructuredTransformResult: ...
def scan_and_transform_structured(
    data: _JsonDocument, config: _StructuredScanAndTransformConfig
) -> StructuredTransformResult: ...

# Workspace result dictionaries retain the shared contract's camelCase fields.
# Revisions are Python integers and preserve the full Core uint64 range.
class _WorkspaceRange(TypedDict):
    start: int
    end: int
class _WorkspaceHeadValue(TypedDict):
    revision: int
    commitId: list[int]
class _WorkspaceMarkerIssue(TypedDict):
    kind: Literal["unknown", "damaged", "unbracketed", "not_issued_in_conversation"]
    byteRange: _WorkspaceRange
    codepointRange: _WorkspaceRange
    utf16Range: _WorkspaceRange
class _WorkspaceReviewConflict(TypedDict):
    kind: Literal["protected_coverage", "type_conflict"]
    findingIndex: int
    byteRange: _WorkspaceRange
    codepointRange: _WorkspaceRange
    utf16Range: _WorkspaceRange
class DataFogWorkspaceError(ValueError):
    code: str
    finding_index: int | None
    input_index: int | None
    marker_issues: list[_WorkspaceMarkerIssue]
    review_conflicts: list[_WorkspaceReviewConflict]
class _WorkspaceRestoreProgress(TypedDict):
    bytes: int
    codepoints: int
    utf16: int
    restoredMarkers: int
class _WorkspaceRestoreChunk(TypedDict):
    text: str
    delivered: _WorkspaceRestoreProgress
    complete: bool
class DataFogWorkspaceStreamError(DataFogWorkspaceError):
    delivered: _WorkspaceRestoreProgress
    complete: Literal[False]
@final
class WorkspaceBufferedRestore:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    def feed(self, value: bytes) -> None: ...
    def feed_utf16(self, value: list[int]) -> None: ...
    def finalize(self) -> _WorkspaceRestoreReport: ...
    def reset(self) -> None: ...
    def close(self) -> None: ...
    def __enter__(self) -> WorkspaceBufferedRestore: ...
    def __exit__(self, kind: object, value: object, traceback: object) -> bool: ...
@final
class WorkspaceProgressiveRestore:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    def feed(self, value: bytes) -> _WorkspaceRestoreChunk: ...
    def feed_utf16(self, value: list[int]) -> _WorkspaceRestoreChunk: ...
    def finalize(self) -> _WorkspaceRestoreChunk: ...
    def reset(self) -> None: ...
    def close(self) -> None: ...
    def __enter__(self) -> WorkspaceProgressiveRestore: ...
    def __exit__(self, kind: object, value: object, traceback: object) -> bool: ...
@final
class WorkspaceHead:
    def __init__(self, revision: int, commit_id: bytes) -> None: ...
    @property
    def revision(self) -> int: ...
    @property
    def commit_id(self) -> bytes: ...
    def __eq__(self, other: object) -> bool: ...
@final
class WorkspaceFinding:
    def __init__(self, entity_type: str, matched_text: str, byte_range: TextRange, *, confidence: float | None = None, detector_name: str | None = None, detector_version: str | None = None, source: Literal["scanner", "user", "recall"] = "scanner", needs_review: bool = True) -> None: ...
    @staticmethod
    def from_finding(finding: Finding) -> WorkspaceFinding: ...
    @property
    def entity_type(self) -> str: ...
    @property
    def matched_text(self) -> str: ...
    @property
    def byte_range(self) -> TextRange: ...
    @property
    def confidence(self) -> float | None: ...
    @property
    def detector_name(self) -> str | None: ...
    @property
    def detector_version(self) -> str | None: ...
    @property
    def source(self) -> Literal["scanner", "user", "recall"]: ...
    @property
    def needs_review(self) -> bool: ...
@final
class WorkspaceInput:
    def __init__(self, text: str, findings: list[WorkspaceFinding]) -> None: ...
@final
class WorkspaceReviewCorrection:
    def __init__(self, candidate_key: str, replacement: WorkspaceFinding) -> None: ...
@final
class WorkspaceReviewInput:
    def __init__(self, text: str, *, scanner_findings: list[WorkspaceFinding] | None = None, user_selections: list[WorkspaceFinding] | None = None, corrections: list[WorkspaceReviewCorrection] | None = None) -> None: ...
class _WorkspaceTransformation(TypedDict):
    source: Literal["scanner", "user", "recall"]
    needsReview: bool
    sourceByteRange: _WorkspaceRange
    sourceCodepointRange: _WorkspaceRange
    sourceUtf16Range: _WorkspaceRange
    outputByteRange: _WorkspaceRange
    outputCodepointRange: _WorkspaceRange
    outputUtf16Range: _WorkspaceRange
class _WorkspacePreview(TypedDict):
    text: str
    transformations: list[_WorkspaceTransformation]
class _WorkspaceSnapshotFormat(TypedDict):
    currentVersion: Literal[1, 2, 3]
    candidateVersion: Literal[1, 2, 3]
class _WorkspaceRuleEdit(TypedDict):
    entityType: str
    rule: Literal["hide", "keep"] | None
class _WorkspaceChoiceEdit(TypedDict):
    entityType: str
    value: str
    disposition: Literal["keep", "not_sensitive"] | None
class _WorkspacePolicyEdits(TypedDict):
    rules: list[_WorkspaceRuleEdit]
    choices: list[_WorkspaceChoiceEdit]
class _WorkspaceRule(TypedDict):
    entityType: str
    rule: Literal["hide", "keep"]
class _WorkspaceChoice(TypedDict):
    entityType: str
    value: str
    disposition: Literal["keep", "not_sensitive"]
class _WorkspacePolicy(TypedDict):
    snapshotVersion: Literal[1, 2, 3]
    defaultRule: Literal["hide"]
    protectedTypes: list[str]
    rules: list[_WorkspaceRule]
    choices: list[_WorkspaceChoice]
class _WorkspaceRulePreview(TypedDict):
    entityType: str
    before: Literal["hide", "keep"] | None
    after: Literal["hide", "keep"] | None
class _WorkspaceChoicePreview(TypedDict):
    entityType: str
    value: str
    before: Literal["keep", "not_sensitive"] | None
    after: Literal["keep", "not_sensitive"] | None
class _WorkspacePolicyPreview(TypedDict):
    rules: list[_WorkspaceRulePreview]
    choices: list[_WorkspaceChoicePreview]
class _WorkspaceFindingRange(_WorkspaceRange):
    unit: Literal["utf8_bytes"]
class _WorkspaceFindingValue(TypedDict):
    entityType: str
    matchedText: str
    range: _WorkspaceFindingRange
    confidence: float | None
    detectorName: str | None
    detectorVersion: str | None
    source: Literal["scanner", "user", "recall"]
    needsReview: bool
class _WorkspaceRecallFinding(TypedDict):
    finding: _WorkspaceFindingValue
    byteRange: _WorkspaceRange
    codepointRange: _WorkspaceRange
    utf16Range: _WorkspaceRange
class _WorkspaceRecall(TypedDict):
    head: _WorkspaceHeadValue
    complete: Literal[True]
    findings: list[_WorkspaceRecallFinding]
class _WorkspaceReviewCandidate(_WorkspaceRecallFinding):
    key: str
    sources: list[Literal["scanner", "user", "recall"]]
    eligible: bool
    selected: bool
class _WorkspaceReview(TypedDict):
    head: _WorkspaceHeadValue
    candidates: list[_WorkspaceReviewCandidate]
    conflicts: list[_WorkspaceReviewConflict]
class _WorkspaceRestoreReport(TypedDict):
    text: str | None
    restoredMarkers: int
    issues: list[_WorkspaceMarkerIssue]
class _WorkspaceOperation(TypedDict):
    id: bytes
    occurredAtMs: int
class _WorkspaceOutcome(TypedDict):
    kind: Literal["copy_confirmed", "user_cancelled", "blocked_input", "restore_blocked"]
    issueCount: int
class _WorkspaceConversation(TypedDict):
    id: bytes | list[int]
    surface: str
    siteId: str | None
    siteIdByteBudget: int
class _WorkspaceConversationInput(TypedDict):
    input: WorkspaceReviewInput
    conversation: _WorkspaceConversation
class _WorkspaceSendIntent(TypedDict):
    preparationId: bytes | list[int]
    inputIndex: int
class _WorkspaceConversationState(TypedDict):
    conversation: _WorkspaceConversation
    pendingIntents: int
    issuedMarkers: int
class _WorkspaceOperationRecord(TypedDict):
    id: list[int]
    kind: Literal["create", "prepare", "recovery", "policy_change", "conversation_prepare", "send_confirmed", "send_not_sent", "copy_confirmed", "user_cancelled", "blocked_input", "restore_blocked"]
    occurredAtMs: int | None
    preparedInputs: int
    transformedSpans: int
class _WorkspaceOperationPage(TypedDict):
    records: list[_WorkspaceOperationRecord]
    total: int
    offset: int
    nextOffset: int | None
class _WorkspaceHistoryCoverage(TypedDict):
    recordedHistoryOnly: Literal[True]
    preUpgradeHistory: Literal["unknown", "not_applicable"]
class _WorkspaceCounts(TypedDict):
    operations: int
    preparedInputs: int
    transformedSpans: int
    confirmedHandoffs: int
    reportedBlockedAttempts: int
@final
class WorkspacePreparedChange:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    def preview(self) -> _WorkspacePreview: ...
    def previews(self) -> list[_WorkspacePreview]: ...
    def expected_head(self) -> WorkspaceHead: ...
    def candidate_head(self) -> WorkspaceHead: ...
    def snapshot_format(self) -> _WorkspaceSnapshotFormat: ...
    def policy_preview(self) -> _WorkspacePolicyPreview | None: ...
    def send_intents(self) -> list[_WorkspaceSendIntent]: ...
    def export_candidate_bundle(self) -> bytes: ...
    def discard(self) -> None: ...
    def __enter__(self) -> WorkspacePreparedChange: ...
    def __exit__(self, kind: object, value: object, traceback: object) -> bool: ...
@final
class Workspace:
    def __init__(self, _unconstructible: NoReturn) -> None: ...
    @staticmethod
    def create(namespace: bytes, operation_id: bytes) -> Workspace: ...
    @staticmethod
    def create_with_operation(namespace: bytes, operation: _WorkspaceOperation) -> Workspace: ...
    @staticmethod
    def open_bundle(bundle: bytes) -> Workspace: ...
    def export_bundle(self) -> bytes: ...
    def head(self) -> WorkspaceHead: ...
    def namespace(self) -> bytes: ...
    def prepare(self, text: str, findings: list[WorkspaceFinding], operation_id: bytes) -> WorkspacePreparedChange: ...
    def prepare_batch(self, inputs: list[WorkspaceInput], operation_id: bytes) -> WorkspacePreparedChange: ...
    def prepare_with_operation(self, text: str, findings: list[WorkspaceFinding], operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def prepare_batch_with_operation(self, inputs: list[WorkspaceInput], operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def prepare_recovery_with_operation(self, operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def prepare_outcome(self, outcome: _WorkspaceOutcome, operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def prepare_conversation_send(self, inputs: list[_WorkspaceConversationInput], operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def conversation(self, logical_id: bytes) -> _WorkspaceConversationState: ...
    def conversation_for_site(self, surface: str, site_id: str) -> bytes: ...
    def send_intent_status(self, intent: _WorkspaceSendIntent) -> Literal["pending", "observed_sent", "definitely_not_sent"]: ...
    def prepare_observed_send(self, intent: _WorkspaceSendIntent, logical_id: bytes, surface: str, site_id: str, operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def prepare_definitely_not_sent(self, intent: _WorkspaceSendIntent, logical_id: bytes, operation: _WorkspaceOperation) -> WorkspacePreparedChange: ...
    def restore_in_conversation(self, text: str, logical_id: bytes) -> str: ...
    def restore_report_in_conversation(self, text: str, logical_id: bytes) -> _WorkspaceRestoreReport: ...
    def buffered_restore_in_conversation(self, logical_id: bytes, encoding: Literal["utf8", "utf16"] = "utf8") -> WorkspaceBufferedRestore: ...
    def progressive_restore_in_conversation(self, logical_id: bytes, encoding: Literal["utf8", "utf16"] = "utf8") -> WorkspaceProgressiveRestore: ...
    def operations(self) -> list[_WorkspaceOperationRecord]: ...
    def operations_page(self, offset: int, limit: int) -> _WorkspaceOperationPage: ...
    def counts(self) -> _WorkspaceCounts: ...
    def history_coverage(self) -> _WorkspaceHistoryCoverage: ...
    def review(self, input: WorkspaceReviewInput) -> _WorkspaceReview: ...
    def prepare_review(self, inputs: list[WorkspaceReviewInput], operation_id: bytes) -> WorkspacePreparedChange: ...
    def prepare_recovery(self, operation_id: bytes) -> WorkspacePreparedChange: ...
    def policy(self) -> _WorkspacePolicy: ...
    def prepare_policy_changes(self, edits: _WorkspacePolicyEdits, operation_id: bytes) -> WorkspacePreparedChange: ...
    def activate_saved(self, change: WorkspacePreparedChange, expected_head: WorkspaceHead, saved_head: WorkspaceHead) -> None: ...
    def buffered_restore(self, encoding: Literal["utf8", "utf16"] = "utf8") -> WorkspaceBufferedRestore: ...
    def progressive_restore(self, encoding: Literal["utf8", "utf16"] = "utf8") -> WorkspaceProgressiveRestore: ...
    def restore(self, text: str) -> str: ...
    def restore_report(self, text: str) -> _WorkspaceRestoreReport: ...
    def recall(self, text: str) -> _WorkspaceRecall: ...
    def close(self) -> None: ...
    def __enter__(self) -> Workspace: ...
    def __exit__(self, kind: object, value: object, traceback: object) -> bool: ...


def workspace_contract_version() -> int: ...
