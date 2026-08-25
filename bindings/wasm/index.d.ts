export type Label =
  | "EMAIL"
  | "PHONE"
  | "SSN"
  | "CREDIT_CARD"
  | "IP_ADDRESS"
  | "DATE"
  | "ZIP_CODE";

export interface Entity {
  readonly label: Label;
  readonly text: string;
  readonly start: number;
  readonly end: number;
}

export function init(): Promise<void>;
export function scan(text: string): Entity[];
