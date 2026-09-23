import { t } from "../i18n";

export type AgentMode = "forbidden" | "manual" | "partial" | "all";
export type AgentBaseMode = "current" | "last_plan";

export interface AgentProviderProfile {
  id: string;
  name: string;
  endpoint: string;
  model: string;
  timeoutMs: number;
}

export interface AgentPromptTemplate {
  id: string;
  name: string;
  instructions: string;
}

/** Endpoint + model pair an operator can apply with one click. */
export interface AgentProviderPreset {
  id: string;
  labelKey: string;
  endpoint: string;
  model: string;
}

/**
 * Known OpenAI-compatible services.
 *
 * Presets only fill the endpoint and model of the active profile; they never
 * create a profile or touch a credential.
 */
export const PROVIDER_PRESETS: ReadonlyArray<AgentProviderPreset> = [
  {
    id: "deepseek",
    labelKey: "agent.preset.deepseek",
    endpoint: "https://api.deepseek.com/v1/chat/completions",
    model: "deepseek-chat",
  },
  {
    id: "openai",
    labelKey: "agent.preset.openai",
    endpoint: "https://api.openai.com/v1/chat/completions",
    model: "gpt-4o-mini",
  },
  {
    id: "ollama",
    labelKey: "agent.preset.ollama",
    endpoint: "http://localhost:11434/v1/chat/completions",
    model: "llama3.1",
  },
  {
    id: "lmstudio",
    labelKey: "agent.preset.lmstudio",
    endpoint: "http://localhost:1234/v1/chat/completions",
    model: "local-model",
  },
];

/** Built-in templates are addressed by a reserved id prefix. */
export const BUILTIN_TEMPLATE_PREFIX = "builtin.";

/**
 * Prompt templates every Studio ships with.
 *
 * They are virtual: nothing is written to localStorage, so they follow the
 * active language, cannot be deleted by accident, and never need a migration.
 * Their text is sent to the model as an operator constraint, exactly like a
 * template the operator wrote.
 */
const BUILTIN_TEMPLATES: ReadonlyArray<{ id: string; nameKey: string; instructionsKey: string }> = [
  {
    id: `${BUILTIN_TEMPLATE_PREFIX}observe-first`,
    nameKey: "agent.template.observeFirst.name",
    instructionsKey: "agent.template.observeFirst.instructions",
  },
  {
    id: `${BUILTIN_TEMPLATE_PREFIX}minimal-change`,
    nameKey: "agent.template.minimalChange.name",
    instructionsKey: "agent.template.minimalChange.instructions",
  },
  {
    id: `${BUILTIN_TEMPLATE_PREFIX}robust-ui`,
    nameKey: "agent.template.robustUi.name",
    instructionsKey: "agent.template.robustUi.instructions",
  },
  {
    id: `${BUILTIN_TEMPLATE_PREFIX}verifiable`,
    nameKey: "agent.template.verifiable.name",
    instructionsKey: "agent.template.verifiable.instructions",
  },
];

/** True when `id` addresses a template the Studio provides. */
export function isBuiltinTemplate(id: string): boolean {
  return id.startsWith(BUILTIN_TEMPLATE_PREFIX);
}

/** Localized built-in templates, in display order. */
export function builtinTemplates(): AgentPromptTemplate[] {
  return BUILTIN_TEMPLATES.map(({ id, nameKey, instructionsKey }) => ({
    id,
    name: t(nameKey),
    instructions: t(instructionsKey),
  }));
}

/** Built-in templates followed by the operator's own, in display order. */
export function allTemplates(userTemplates: AgentPromptTemplate[]): AgentPromptTemplate[] {
  return [...builtinTemplates(), ...userTemplates];
}

/** Resolve a template id against the built-ins and the operator's templates. */
export function findTemplate(
  id: string | undefined,
  userTemplates: AgentPromptTemplate[],
  builtins: AgentPromptTemplate[] = builtinTemplates(),
): AgentPromptTemplate | undefined {
  if (!id) return undefined;
  return builtins.find((template) => template.id === id)
    ?? userTemplates.find((template) => template.id === id);
}

export interface AgentSettings {
  version: 1;
  activeProfileId: string;
  profiles: AgentProviderProfile[];
  mode: AgentMode;
  baseMode: AgentBaseMode;
  extraInstructions: string;
  templates: AgentPromptTemplate[];
  workspaceOpen: boolean;
}

const STORAGE_KEY = "nodara.agent.settings.v1";

export function defaultAgentSettings(): AgentSettings {
  const profile: AgentProviderProfile = {
    id: "default",
    name: "OpenAI compatible",
    endpoint: "https://api.openai.com/v1/chat/completions",
    model: "gpt-4o-mini",
    timeoutMs: 300_000,
  };
  return {
    version: 1,
    activeProfileId: profile.id,
    profiles: [profile],
    mode: "partial",
    baseMode: "current",
    extraInstructions: "",
    templates: [],
    workspaceOpen: false,
  };
}

export function loadAgentSettings(): AgentSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaultAgentSettings();
    const parsed = JSON.parse(raw) as Partial<AgentSettings>;
    const defaults = defaultAgentSettings();
    const profiles = Array.isArray(parsed.profiles) && parsed.profiles.length > 0
      ? parsed.profiles.filter(isProfile)
      : defaults.profiles;
    const activeProfileId = profiles.some((profile) => profile.id === parsed.activeProfileId)
      ? String(parsed.activeProfileId)
      : profiles[0].id;
    return {
      version: 1,
      activeProfileId,
      profiles,
      mode: isMode(parsed.mode) ? parsed.mode : defaults.mode,
      baseMode: parsed.baseMode === "last_plan" ? "last_plan" : "current",
      extraInstructions: typeof parsed.extraInstructions === "string" ? parsed.extraInstructions : "",
      templates: Array.isArray(parsed.templates)
        ? parsed.templates.filter(isTemplate)
        : [],
      workspaceOpen: parsed.workspaceOpen === true,
    };
  } catch {
    return defaultAgentSettings();
  }
}

export function saveAgentSettings(settings: AgentSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Storage is optional; settings remain live for this process.
  }
}

export function newProfile(index: number): AgentProviderProfile {
  return {
    id: globalThis.crypto?.randomUUID?.() ?? `profile-${Date.now().toString(36)}`,
    name: `Provider ${index}`,
    endpoint: "https://api.openai.com/v1/chat/completions",
    model: "gpt-4o-mini",
    timeoutMs: 300_000,
  };
}

export function newTemplate(index: number): AgentPromptTemplate {
  return {
    id: globalThis.crypto?.randomUUID?.() ?? `template-${Date.now().toString(36)}`,
    name: `Template ${index}`,
    instructions: "",
  };
}

function isProfile(value: unknown): value is AgentProviderProfile {
  if (typeof value !== "object" || value === null) return false;
  const profile = value as Partial<AgentProviderProfile>;
  return typeof profile.id === "string"
    && typeof profile.name === "string"
    && typeof profile.endpoint === "string"
    && typeof profile.model === "string"
    && typeof profile.timeoutMs === "number";
}

function isTemplate(value: unknown): value is AgentPromptTemplate {
  if (typeof value !== "object" || value === null) return false;
  const template = value as Partial<AgentPromptTemplate>;
  return typeof template.id === "string"
    && typeof template.name === "string"
    && typeof template.instructions === "string";
}

function isMode(value: unknown): value is AgentMode {
  return value === "forbidden" || value === "manual" || value === "partial" || value === "all";
}
