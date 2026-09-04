/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;
export type TransformationStrategy = "redact" | "mask" | "remove" | "tokenize";

export interface MaskRevealConfig {
  readonly direction: "first" | "last";
  readonly count: number;
}

export type TransformationStrategyConfig =
  | { readonly strategy: "redact" }
  | { readonly strategy: "remove" }
  | { readonly strategy: "tokenize"; readonly token_ref: string }
  | {
      readonly strategy: "mask";
      readonly character?: string;
      readonly reveal?: MaskRevealConfig;
    };

export interface RegexAllowRule {
  readonly pattern: string;
  readonly case_sensitive?: boolean;
}

export interface AllowConfig {
  readonly exact?: Readonly<Record<EntityType, readonly string[]>>;
  readonly regex?: Readonly<Record<EntityType, readonly RegexAllowRule[]>>;
}

export interface TransformationConfig {
  readonly default: TransformationStrategyConfig;
  readonly entities?: readonly EntityType[];
  readonly overrides?: Readonly<Record<EntityType, TransformationStrategyConfig>>;
  readonly allow?: AllowConfig;
}

export interface ScanConfig {
  readonly locale?: string;
}

export interface ScanAndTransformConfig {
  readonly scan?: ScanConfig;
  readonly transform: TransformationConfig;
}

export type DataFogErrorCode =
  | "invalid_configuration"
  | "invalid_finding"
  | "key_provider_required"
  | "key_not_found"
  | "key_access_denied"
  | "key_provider_unavailable"
  | "invalid_key_material"
  | "key_provider_error"
  | "token_provider_required"
  | "invalid_token"
  | "unsupported_token_version"
  | "token_not_found"
  | "token_expired"
  | "token_access_denied"
  | "invalid_token_material"
  | "token_provider_unavailable"
  | "token_provider_error"
  | "unsupported_strategy"
  | "internal_error";

export declare class DataFogError extends Error {
  readonly code: DataFogErrorCode;
  readonly reason?: string;
  readonly path?: string;
  readonly findingIndex?: number;
}

export interface TextRange {
  readonly start: number;
  readonly end: number;
}

export interface FindingInput {
  readonly entityType: EntityType;
  readonly matchedText: string;
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly confidence?: number;
  readonly detectorName: string;
  readonly detectorVersion?: string;
}

export interface Finding extends FindingInput {
  readonly utf16Range: TextRange;
}

export interface Transformation {
  readonly entityType: EntityType;
  readonly sourceByteRange: TextRange;
  readonly sourceCodepointRange: TextRange;
  readonly sourceUtf16Range: TextRange;
  readonly confidence?: number;
  readonly detectorName: string;
  readonly detectorVersion?: string;
  readonly strategy: TransformationStrategy;
  readonly replacement: string;
  readonly outputByteRange: TextRange;
  readonly outputCodepointRange: TextRange;
  readonly outputUtf16Range: TextRange;
  readonly keyRef?: string;
  readonly resolvedKeyVersion?: string;
  readonly tokenRef?: string;
  readonly resolvedTokenVersion?: string;
}

export interface TransformResult {
  readonly text: string;
  readonly transformations: Transformation[];
}

export function init(): Promise<void>;
export function scan(text: string, config?: ScanConfig): Finding[];
export function transform(
  text: string,
  findings: FindingInput[],
  config: TransformationConfig,
): TransformResult;
export function scanAndTransform(
  text: string,
  config: ScanAndTransformConfig,
): TransformResult;
export interface PrivacyContext { readonly scope: string; }
export interface Restoration {
  readonly sourceByteRange: TextRange;
  readonly sourceCodepointRange: TextRange;
  readonly sourceUtf16Range: TextRange;
  readonly outputByteRange: TextRange;
  readonly outputCodepointRange: TextRange;
  readonly outputUtf16Range: TextRange;
  readonly tokenRef: string;
  readonly resolvedTokenVersion: string;
}
export interface RestoreResult { readonly text: string; readonly restorations: Restoration[]; }
export function restore(text: string, context: PrivacyContext): RestoreResult;

/** JSON input uses finite numbers; integer values must be JavaScript-safe. */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type JsonDocument = JsonValue[] | { [key: string]: JsonValue };
export interface StructuredScanConfig {
  readonly locale?: string;
  readonly discover_person?: boolean;
  readonly mappings?: Readonly<Record<string, "PERSON">>;
  readonly exclude?: readonly string[];
}

export interface FieldMapping {
  readonly path: string;
  readonly entityType: "PERSON";
  readonly source: "field_alias" | "explicit_mapping";
  readonly rule: string;
}
export interface StructuredFinding { readonly path: string; readonly finding: Finding; }
export interface StructuredScanResult { readonly mappings: FieldMapping[]; readonly findings: StructuredFinding[]; }
export function discoverFields(data: JsonDocument, config?: StructuredScanConfig): FieldMapping[];
export function scanStructured(data: JsonDocument, config?: StructuredScanConfig): StructuredScanResult;

export interface StructuredScanAndTransformConfig { readonly scan?: StructuredScanConfig; readonly transform: TransformationConfig; }
export interface StructuredFindingInput { readonly path: string; readonly finding: FindingInput; }
export interface StructuredTransformation { readonly path: string; readonly transformation: Transformation; }
export interface StructuredTransformResult { readonly data: JsonDocument; readonly transformations: StructuredTransformation[]; }
export function transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig): StructuredTransformResult;
export function scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig): StructuredTransformResult;
/** Always rejects with unsupported_strategy after input validation. */
export function restoreStructured(data: JsonDocument, context: PrivacyContext): never;
