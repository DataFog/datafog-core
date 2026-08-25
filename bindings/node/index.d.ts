export type Label =
  | "EMAIL"
  | "PHONE"
  | "SSN"
  | "CREDIT_CARD"
  | "IP_ADDRESS"
  | "DATE"
  | "ZIP_CODE";
export interface Entity {
  readonly label: Label
  readonly text: string
  readonly start: number
  readonly end: number
}

/** Scan text for supported PII entities. */
export declare function scan(text: string): Array<Entity>
