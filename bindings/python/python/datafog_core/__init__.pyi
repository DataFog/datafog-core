"""Types for the compiled API. Underscored helpers exist only for type checking."""

from collections.abc import Awaitable, Iterable, Sequence
from typing import Literal, NoReturn, Protocol, TypeAlias, TypedDict, final

_JsonValue: TypeAlias = (
    None | bool | int | float | str | list["_JsonValue"] | dict[str, "_JsonValue"]
)
_JsonDocument: TypeAlias = list[_JsonValue] | dict[str, _JsonValue]

class _ScanConfig(TypedDict, total=False):
    locale: str

class _StructuredScanConfig(_ScanConfig, total=False):
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
