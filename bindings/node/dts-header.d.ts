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
