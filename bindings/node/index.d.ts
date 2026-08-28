/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

export type TransformationStrategy = "redact";
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
export declare function scan(text: string): Array<Finding>

/** Scan text and transform the detected findings. */
export declare function scanAndTransform(text: string, strategy: TransformationStrategy): TransformResult

export interface TextRange {
  readonly start: number
  readonly end: number
}

/** Transform explicit findings without scanning implicitly. */
export declare function transform(text: string, findings: Array<Finding>, strategy: TransformationStrategy): TransformResult

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
