// The app skin contract, shared with the server (crates/plugin-api/skin-rules.json).
// The server checks skins with the same file, so both sides agree on what's allowed.

import rulesJson from "../../../crates/plugin-api/skin-rules.json";

export interface BindingRule {
  value: string;
  values?: string[];
  help: string;
}

export interface SkinRules {
  skinApi: number;
  supported: number[];
  limits: { textBytes: number; assetBytes: number; files: number; partialDepth: number; cardElements: number };
  devices: string[];
  pages: Record<string, { scope: string; variants: string[] }>;
  assetTypes: Record<string, string>;
  tags: string[];
  tagsNeedBinding: Record<string, string>;
  voidTags: string[];
  attributes: { global: string[]; prefixes: string[]; perTag: Record<string, string[]>; svg: string[] };
  bindings: Record<string, BindingRule>;
  renamed: { bindings: Record<string, string>; fields: Record<string, string> };
  imageKinds: string[];
  imageWidths: number[];
  loopHelpers: string[];
  types: Record<string, Record<string, string>>;
}

export const RULES = rulesJson as unknown as SkinRules;

export const ALLOWED_TAGS = new Set(RULES.tags);
