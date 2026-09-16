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
