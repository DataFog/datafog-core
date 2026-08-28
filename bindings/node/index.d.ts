/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;
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

export interface TextRange {
  readonly start: number
  readonly end: number
}
