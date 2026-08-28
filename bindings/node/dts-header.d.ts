/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

export type TransformationStrategy =
  | "redact"
  | "mask"
  | "remove"
  | "pseudonymize";

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

export declare class PrivacyManager {
  constructor(provider: KeyProvider);
  transform(
    text: string,
    findings: Finding[],
    config: TransformationConfig,
  ): Promise<TransformResult>;
  scanAndTransform(
    text: string,
    config: ScanAndTransformConfig,
  ): Promise<TransformResult>;
}
