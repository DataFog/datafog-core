/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;
export type TransformationStrategy = "redact";

export interface TextRange {
  readonly start: number;
  readonly end: number;
}

export interface Finding {
  readonly entityType: EntityType;
  readonly matchedText: string;
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly confidence?: number;
  readonly detectorName: string;
  readonly detectorVersion?: string;
}

export interface Transformation {
  readonly finding: Finding;
  readonly strategy: TransformationStrategy;
  readonly replacement: string;
  readonly outputByteRange: TextRange;
  readonly outputCodepointRange: TextRange;
}

export interface TransformResult {
  readonly text: string;
  readonly transformations: Transformation[];
}

export function init(): Promise<void>;
export function scan(text: string): Finding[];
export function transform(
  text: string,
  findings: Finding[],
  strategy: TransformationStrategy,
): TransformResult;
export function scanAndTransform(
  text: string,
  strategy: TransformationStrategy,
): TransformResult;
