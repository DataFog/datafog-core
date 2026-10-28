/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

export interface ModelSessionConfig {
  /** Omit to retain legacy selection from the presence of gliner2 settings. */
  engine?: "datafog" | "gliner2";
  bundle: string;
  manifest_sha256: string;
  limits: {
    max_input_bytes: number;
    max_response_bytes: number;
    max_findings: number;
    timeout_ms: number;
    cleanup_timeout_ms: number;
  };
  gliner2?: {
    python: string;
    labels: Array<{label: string; threshold: number; description?: string}>;
    intra_threads: number;
    chunk_size: number;
    chunk_overlap: number;
  };
}
export interface ModelFindingEvidence {
  finding_index: number;
  sources: Array<{
    detector_name: string;
    detector_version: string | null;
    native_label: string | null;
    raw_score: {value: number; scale_id: string} | null;
    calibrated_confidence: number | null;
  }>;
}
export interface ModelDescriptor {
  readonly backend: string;
  readonly model: string;
  readonly adapter_version: string;
  /** Native model label → configured canonical entity type. */
  readonly labels: Readonly<Record<string, string>>;
  readonly score_scale: string;
}
/** Rule scan options; `entities` may also name the session's model labels. */
export interface ModelScanConfig extends Omit<ScanConfig, "entities"> {
  readonly entities?: readonly string[];
}
export declare class ModelSession {
  private constructor();
  static open(config: ModelSessionConfig): Promise<ModelSession>;
  /** Configured metadata, available after close; does not probe runtime readiness. */
  describe(): ModelDescriptor;
  /** Rule detectors handle the entity types they support and the model runs
   * only for requested types it alone provides; rule-only requests skip the
   * model. Omitting `config` equals `{}`: every enabled rule detector plus the
   * model's remaining labels. Unknown option keys throw a TypeError. */
  scan(text: string, options?: {config?: ModelScanConfig; cancellation?: ModelCancellation}): Promise<ModelScanResult>;
  /** Raw model output: every model label, with no rule detectors. */
  scanModel(text: string, options?: {cancellation?: ModelCancellation}): Promise<ModelScanResult>;
  close(): Promise<void>;
}

export type TransformationStrategy =
  | "redact"
  | "mask"
  | "remove"
  | "pseudonymize"
  | "tokenize";

export interface MaskRevealConfig {
  readonly direction: "first" | "last";
  readonly count: number;
}

export type TransformationStrategyConfig =
  | { readonly strategy: "redact" }
  | { readonly strategy: "remove" }
  | {
      readonly strategy: "pseudonymize";
      readonly key_ref: string;
      readonly key_version?: string;
    }
  | {
      readonly strategy: "tokenize";
      readonly token_ref: string;
    }
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
  /** Nonempty unique text entity types to run; omission runs all activated detectors.
   * Selection does not activate locale-dependent types or UUID detection. */
  readonly entities?: readonly EntityType[];
  /** Source syntax for email boundaries; defaults to text. */
  readonly format?: "text" | "env" | "sql";
  readonly locale?: string;
  /** Opt-in canonical UUID detection; disabled by default. */
  readonly detect_uuid?: boolean;
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

export interface KeyProviderRequest {
  readonly keyRef: string;
  readonly keyVersion?: string;
}

export interface KeyProviderResponse {
  readonly key: Uint8Array;
  readonly resolvedVersion: string;
}

export interface KeyProvider {
  resolveKey(request: KeyProviderRequest): Promise<KeyProviderResponse>;
}

export interface PrivacyContext {
  readonly scope: string;
}

export interface TokenizeProviderItem {
  readonly id: string;
  readonly exactValue: string;
  readonly tokenRef: string;
}

export interface TokenizeProviderResult {
  readonly id: string;
  readonly payload: Uint8Array;
  readonly resolvedVersion: string;
}

export interface RestoreProviderItem {
  readonly id: string;
  readonly tokenRef: string;
  readonly resolvedVersion: string;
  readonly payload: Uint8Array;
}

export interface RestoreProviderResult {
  readonly id: string;
  readonly value: string;
}

export interface TokenProvider {
  tokenizeBatch(scope: string, items: TokenizeProviderItem[]): Promise<TokenizeProviderResult[]>;
  restoreBatch(scope: string, items: RestoreProviderItem[]): Promise<RestoreProviderResult[]>;
}

export interface PrivacyManagerProviders {
  readonly keyProvider?: KeyProvider;
  readonly tokenProvider?: TokenProvider;
}

export declare class PrivacyManager {
  transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig, context?: PrivacyContext): Promise<StructuredTransformResult>;
  scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig, context?: PrivacyContext): Promise<StructuredTransformResult>;
  restoreStructured(data: JsonDocument, context: PrivacyContext): Promise<StructuredRestoreResult>;

  constructor(provider: KeyProvider | PrivacyManagerProviders, tokenProvider?: TokenProvider);
  transform(
    text: string,
    findings: FindingInput[],
    config: TransformationConfig,
    context?: PrivacyContext,
  ): Promise<TransformResult>;
  scanAndTransform(
    text: string,
    config: ScanAndTransformConfig,
    context?: PrivacyContext,
  ): Promise<TransformResult>;
  restore(text: string, context: PrivacyContext): Promise<RestoreResult>;
}

/** JSON input uses finite numbers; integer values must be JavaScript-safe. */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type JsonDocument = JsonValue[] | { [key: string]: JsonValue };
export interface StructuredScanConfig {
  /** Source syntax for email boundaries in each string leaf; defaults to text. */
  readonly format?: "text" | "env" | "sql";
  readonly locale?: string;
  /** Opt-in canonical UUID detection; disabled by default. */
  readonly detect_uuid?: boolean;
  readonly discover_person?: boolean;
  readonly mappings?: Readonly<Record<string, "PERSON">>;
  readonly exclude?: readonly string[];
}

export interface StructuredScanAndTransformConfig {
  readonly scan?: StructuredScanConfig;
  readonly transform: TransformationConfig;
}
export declare function transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig): StructuredTransformResult;
export declare function scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig): StructuredTransformResult;

export interface StructuredTransformResult { readonly data: JsonDocument; readonly transformations: StructuredTransformation[]; }
export interface StructuredRestoreResult { readonly data: JsonDocument; readonly restorations: StructuredRestoration[]; }
export declare class ModelCancellation {
  constructor()
  cancel(): void
}

export declare class ModelSessionNative {
  describe(): ModelDescriptor
  scan(text: string, cancellation?: ModelCancellation | undefined | null, config?: any | undefined | null): Promise<ModelScanResult>
  /** Raw model output: every model label, with no rule detectors. */
  scanModel(text: string, cancellation?: ModelCancellation | undefined | null): Promise<ModelScanResult>
  close(): Promise<void>
}

export declare function discoverFields(dataJson: JsonDocument, config?: StructuredScanConfig | undefined): Array<FieldMapping>

export interface FieldMapping {
  path: string
  entityType: string
  source: string
  rule: string
}

export interface Finding {
  readonly entityType: EntityType
  readonly matchedText: string
  readonly byteRange: TextRange
  readonly codepointRange: TextRange
  readonly utf16Range: TextRange
  readonly confidence?: number
  readonly detectorName: string
  readonly detectorVersion?: string
}

export interface FindingInput {
  entityType: EntityType
  matchedText: string
  byteRange: TextRange
  codepointRange: TextRange
  confidence?: number
  detectorName: string
  detectorVersion?: string
}

export interface KeySelector {
  readonly index: number
  readonly keyRef: string
  readonly keyVersion?: string
  readonly path: string
}

export interface ModelScanResult {
  contractVersion: number
  model: string
  findings: Array<Finding>
  evidence: ModelFindingEvidence[]
}

export interface NativeStructuredRestoreResult {
  dataJson: string
  restorations: Array<StructuredRestoration>
}

export interface NativeStructuredTransformResult {
  dataJson: string
  transformations: Array<StructuredTransformation>
}

export declare function openModelSession(config: any): Promise<ModelSessionNative>

export interface PreparedScanAndTransform {
  readonly findings: Array<Finding>
  readonly selectors: Array<KeySelector>
}

export interface PreparedStructuredScan {
  findings: Array<StructuredFinding>
  selectors: Array<KeySelector>
}

export declare function prepareScanAndTransform(text: string, config: ScanAndTransformConfig): PreparedScanAndTransform

export declare function prepareStructuredScanAndTransform(text: string, config: unknown): PreparedStructuredScan

export declare function requiredKeySelectors(text: string, findings: FindingInput[], config: TransformationConfig): Array<KeySelector>

export declare function requiredRestoreItems(text: string, context: PrivacyContext): Array<RestoreItem>

export declare function requiredTokenizationItems(text: string, findings: FindingInput[], config: TransformationConfig, context?: PrivacyContext | undefined): Array<TokenizeItem>

export interface ResolvedKeyInput {
  selectorIndex: number
  key: Uint8Array
  resolvedVersion: string
}

export interface Restoration {
  readonly sourceByteRange: TextRange
  readonly sourceCodepointRange: TextRange
  readonly sourceUtf16Range: TextRange
  readonly outputByteRange: TextRange
  readonly outputCodepointRange: TextRange
  readonly outputUtf16Range: TextRange
  readonly tokenRef: string
  readonly resolvedTokenVersion: string
}

export interface RestoredValueInput {
  id: string
  value: string
}

export interface RestoreItem {
  readonly id: string
  readonly tokenRef: string
  readonly resolvedVersion: string
  readonly payload: Uint8Array
}

export interface RestoreResult {
  readonly text: string
  readonly restorations: Array<Restoration>
}

export declare function restoreWithResults(text: string, context: PrivacyContext, results: Array<RestoredValueInput>): RestoreResult

/** Scan text for supported PII findings. */
export declare function scan(text: string, config?: ScanConfig | undefined): Array<Finding>

/** Scan text and transform the detected findings. */
export declare function scanAndTransform(text: string, config: ScanAndTransformConfig): TransformResult

export declare function scanStructured(dataJson: JsonDocument, config?: StructuredScanConfig | undefined): StructuredScanResult

export interface StructuredFinding {
  path: string
  finding: Finding
}

export interface StructuredFindingInput {
  path: string
  finding: FindingInput
}

/** Scan text and transform the detected findings. */
export declare function structuredRequiredKeySelectors(text: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig): Array<KeySelector>

export declare function structuredRequiredRestoreItems(text: JsonDocument, context: PrivacyContext): Array<RestoreItem>

export declare function structuredRequiredTokenizationItems(text: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig, context?: PrivacyContext | undefined): Array<TokenizeItem>

export interface StructuredRestoration {
  path: string
  restoration: Restoration
}

export declare function structuredRestoreWithResults(text: JsonDocument, context: PrivacyContext, results: Array<RestoredValueInput>): NativeStructuredRestoreResult

export declare function structuredScanAndTransform(text: JsonDocument, config: StructuredScanAndTransformConfig): NativeStructuredTransformResult

export interface StructuredScanResult {
  mappings: Array<FieldMapping>
  findings: Array<StructuredFinding>
}

export declare function structuredTransform(text: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig): NativeStructuredTransformResult

export interface StructuredTransformation {
  path: string
  transformation: Transformation
}

export declare function structuredTransformWithProviderResults(text: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig, context: PrivacyContext | undefined, resolvedKeys: Array<ResolvedKeyInput>, tokenResults: Array<TokenizeResultInput>): NativeStructuredTransformResult

export interface TextRange {
  readonly start: number
  readonly end: number
}

export interface TokenizeItem {
  readonly id: string
  readonly exactValue: string
  readonly tokenRef: string
}

export interface TokenizeResultInput {
  id: string
  payload: Uint8Array
  resolvedVersion: string
}

/** Transform explicit findings without scanning implicitly. */
export declare function transform(text: string, findings: FindingInput[], config: TransformationConfig): TransformResult

export interface Transformation {
  readonly entityType: string
  readonly sourceByteRange: TextRange
  readonly sourceCodepointRange: TextRange
  readonly sourceUtf16Range: TextRange
  readonly confidence?: number
  readonly detectorName: string
  readonly detectorVersion?: string
  readonly strategy: TransformationStrategy
  readonly replacement: string
  readonly outputByteRange: TextRange
  readonly outputCodepointRange: TextRange
  readonly outputUtf16Range: TextRange
  readonly keyRef?: string
  readonly resolvedKeyVersion?: string
  readonly tokenRef?: string
  readonly resolvedTokenVersion?: string
}

export interface TransformResult {
  readonly text: string
  readonly transformations: Array<Transformation>
}

export declare function transformWithProviderResults(text: string, findings: FindingInput[], config: TransformationConfig, context: PrivacyContext | undefined, resolvedKeys: Array<ResolvedKeyInput>, tokenResults: Array<TokenizeResultInput>): TransformResult

export declare function transformWithResolvedKeys(text: string, findings: FindingInput[], config: TransformationConfig, resolvedKeys: Array<ResolvedKeyInput>): TransformResult
