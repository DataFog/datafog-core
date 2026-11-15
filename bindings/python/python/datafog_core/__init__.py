"""Fast structured PII detection implemented in Rust."""

from .datafog_core import (
    DataFogWorkspaceError as DataFogWorkspaceError,
    Workspace as Workspace,
    WorkspaceBufferedRestore as WorkspaceBufferedRestore,
    WorkspaceProgressiveRestore as WorkspaceProgressiveRestore,
    DataFogWorkspaceStreamError as DataFogWorkspaceStreamError,
    WorkspaceHead as WorkspaceHead,
    WorkspaceFinding as WorkspaceFinding,
    WorkspaceInput as WorkspaceInput,
    WorkspaceReviewCorrection as WorkspaceReviewCorrection,
    WorkspaceReviewInput as WorkspaceReviewInput,
    WorkspacePreparedChange as WorkspacePreparedChange,
    DataFogModelError as DataFogModelError,
    EmbeddedCnnDetector as EmbeddedCnnDetector,
    ModelSession as ModelSession,
    ModelCancellation as ModelCancellation,
    ModelScanResult as ModelScanResult,
    DataFogConfigurationError as DataFogConfigurationError,
    DataFogFindingError as DataFogFindingError,
    DataFogInternalError as DataFogInternalError,
    DataFogKeyProviderError as DataFogKeyProviderError,
    FieldMapping as FieldMapping,
    Finding as Finding,
    PrivacyManager as PrivacyManager,
    Restoration as Restoration,
    RestoreResult as RestoreResult,
    StructuredFinding as StructuredFinding,
    StructuredRestoration as StructuredRestoration,
    StructuredRestoreResult as StructuredRestoreResult,
    StructuredScanResult as StructuredScanResult,
    StructuredTransformation as StructuredTransformation,
    StructuredTransformResult as StructuredTransformResult,
    TextRange as TextRange,
    Transformation as Transformation,
    TransformResult as TransformResult,
    capabilities as capabilities,
    workspace_contract_version as workspace_contract_version,
    discover_fields as discover_fields,
    scan as scan,
    scan_and_transform as scan_and_transform,
    scan_and_transform_structured as scan_and_transform_structured,
    __version__ as __version__,
    scan_structured as scan_structured,
    transform as transform,
    transform_structured as transform_structured,
)

__all__ = [
    "DataFogWorkspaceError", "Workspace", "WorkspaceHead", "WorkspaceFinding",
    "WorkspaceInput", "WorkspaceReviewCorrection", "WorkspaceReviewInput",
    "WorkspacePreparedChange", "WorkspaceBufferedRestore", "WorkspaceProgressiveRestore",
    "DataFogWorkspaceStreamError",
    "DataFogModelError",
    "EmbeddedCnnDetector",
    "ModelSession",
    "ModelCancellation",
    "ModelScanResult",
    "DataFogConfigurationError",
    "DataFogFindingError",
    "DataFogInternalError",
    "DataFogKeyProviderError",
    "TextRange",
    "Finding",
    "Transformation",
    "TransformResult",
    "Restoration",
    "RestoreResult",
    "FieldMapping",
    "StructuredFinding",
    "StructuredScanResult",
    "StructuredTransformation",
    "StructuredTransformResult",
    "StructuredRestoration",
    "StructuredRestoreResult",
    "PrivacyManager",
    "capabilities",
    "workspace_contract_version",
    "scan",
    "transform",
    "scan_and_transform",
    "discover_fields",
    "scan_structured",
    "transform_structured",
    "scan_and_transform_structured",
]


def panic_probe() -> None:
    """Deprecated compatibility stub; never invokes a Rust panic.

    The original test helper is supported by datafog-core==1.4.0. This stub
    is deprecated in 2.0 and remains until at least 3.0 and the required
    90-day deprecation window.
    """
    import warnings
    warnings.warn(
        "panic_probe is deprecated and no longer invokes a Rust panic; "
        "the original helper is available in datafog-core==1.4.0. "
        "The name will be removed no earlier than 3.0 and the 90-day deprecation window.",
        DeprecationWarning,
        stacklevel=2,
    )
    error = DataFogInternalError("panic_probe is a deprecated test helper")
    error.code = "internal_error"
    error.reason = None
    error.path = None
    error.finding_index = None
    raise error
