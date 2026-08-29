/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

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
