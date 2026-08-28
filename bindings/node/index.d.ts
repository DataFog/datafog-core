/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

export type TransformationStrategy = "redact" | "mask" | "remove";

export interface MaskRevealConfig {
  readonly direction: "first" | "last";
  readonly count: number;
}

export type TransformationStrategyConfig =
  | { readonly strategy: "redact" }
  | { readonly strategy: "remove" }
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
  | "internal_error";

export declare class DataFogError extends Error {
  readonly code: DataFogErrorCode;
  readonly reason?: string;
  readonly path?: string;
  readonly findingIndex?: number;
}
export interface Finding {
  readonly entityType: EntityType
  readonly matchedText: string
  readonly byteRange: TextRange
  readonly codepointRange: TextRange
  readonly confidence?: number
  readonly detectorName: string
  readonly detectorVersion?: string
}

/** Scan text for supported PII findings. */
export declare function scan(text: string, config?: ScanConfig | undefined): Array<Finding>

/** Scan text and transform the detected findings. */
export declare function scanAndTransform(text: string, config: ScanAndTransformConfig): TransformResult

export interface TextRange {
  readonly start: number
  readonly end: number
}

/** Transform explicit findings without scanning implicitly. */
export declare function transform(text: string, findings: Array<Finding>, config: TransformationConfig): TransformResult

export interface Transformation {
  readonly finding: Finding
  readonly strategy: TransformationStrategy
  readonly replacement: string
  readonly outputByteRange: TextRange
  readonly outputCodepointRange: TextRange
}

export interface TransformResult {
  readonly text: string
  readonly transformations: Array<Transformation>
}
