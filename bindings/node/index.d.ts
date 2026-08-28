/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

export type TransformationStrategy = "redact" | "mask" | "remove";

export interface MaskRevealConfig {
  readonly direction: "first" | "last";
  readonly count: number;
}

export type TransformationConfig =
  | { readonly strategy: "redact" }
  | { readonly strategy: "remove" }
  | {
      readonly strategy: "mask";
      readonly character?: string;
      readonly reveal?: MaskRevealConfig;
    };
export interface Finding {
  readonly entityType: EntityType
  readonly matchedText: string
  readonly byteRange: TextRange
  readonly codepointRange: TextRange
  readonly confidence?: number
  readonly detectorName: string
  readonly detectorVersion?: string
}

export interface NativeMaskRevealConfig {
  direction: string
  count: number
}

export interface NativeTransformationConfig {
  strategy: TransformationStrategy
  character?: string
  reveal?: NativeMaskRevealConfig
}

/** Scan text for supported PII findings. */
export declare function scan(text: string): Array<Finding>

/** Scan text and transform the detected findings. */
export declare function scanAndTransform(text: string, config: TransformationConfig): TransformResult

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
