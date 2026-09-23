/**
 * The Agent configuration surface.
 *
 * Provider profiles, credentials, prompt templates and the per-turn extra
 * instructions used to be stacked above the conversation, inside a drawer panel
 * that is often only ~160px tall: the form filled the visible area, the composer
 * overlapped it, and the primary task — describing a goal — was pushed off
 * screen. Everything that is *configuration* now lives here, in a modal dialog
 * that gets the full window height.
 *
 * The dialog owns no durable state of its own: it edits the `AgentSettings`
 * instance the panel owns, reports every change through `onChange`, and keeps
 * credentials in the OS credential store.
 */

import { t } from "../i18n";
import {
  AgentProviderPreset,
  AgentProviderProfile,
  AgentPromptTemplate,
  AgentSettings,
  PROVIDER_PRESETS,
  builtinTemplates,
  newProfile,
  newTemplate,
} from "../model/agent-settings";

export interface AgentSettingsDialogHandlers {
  /** A setting changed and should be persisted. */
  onChange: () => void;
  onCredentialGet: (profileId: string) => Promise<string | null>;
  onCredentialSet: (profileId: string, secret: string) => Promise<void>;
  onCredentialDelete: (profileId: string) => Promise<void>;
}

type CredentialState = "loading" | "saved" | "missing" | "failed";

interface FocusSnapshot {
  field: string;
  start: number | null;
  end: number | null;
}

const TIMEOUT_MIN = 1_000;
const TIMEOUT_MAX = 3_600_000;

/** `http(s)://host/...` — anything else cannot be an OpenAI-compatible endpoint. */
function isEndpoint(value: string): boolean {
  return /^https?:\/\/[^\s]+$/i.test(value.trim());
}

/** The profile the dialog edits. */
function activeProfile(settings: AgentSettings): AgentProviderProfile {
  return settings.profiles.find((profile) => profile.id === settings.activeProfileId)
    ?? settings.profiles[0];
}

/** The preset a profile currently matches, if any. */
function matchingPreset(profile: AgentProviderProfile): AgentProviderPreset | undefined {
  return PROVIDER_PRESETS.find(
    (preset) => preset.endpoint === profile.endpoint && preset.model === profile.model,
  );
}

export class AgentSettingsDialog {
  private readonly root: HTMLDialogElement;
  private settings: AgentSettings | null = null;
  private credential: CredentialState = "missing";
  private apiKey = "";
  private credentialError = "";
  private status = "";
  private confirmPending: string | null = null;
  private opened = false;

  constructor(
    private readonly handlers: AgentSettingsDialogHandlers,
    private readonly desktopAvailable = true,
  ) {
    this.root = document.createElement("dialog");
    this.root.className = "modal agent-settings";
    this.root.id = "agent-settings";
    this.root.setAttribute("aria-label", t("agent.settingsTitle"));
    this.root.addEventListener("click", (event) => {
      if (event.target === this.root) this.close();
    });
    // A modal `<dialog>` also closes on Escape through the user agent; listening
    // here keeps the flag honest for that path and makes the behaviour explicit
    // (and testable) rather than implicit.
    this.root.addEventListener("close", () => {
      this.opened = false;
    });
    this.root.addEventListener("keydown", (event) => {
      if (event.key !== "Escape") return;
      // One Escape closes one thing: without this the event would keep bubbling
      // and collapse the workspace behind the dialog as well.
      event.stopPropagation();
      this.close();
    });
    // One dialog per document: a second panel (a reload, a test) must not leave
    // a stale duplicate behind an id lookup.
    document.getElementById("agent-settings")?.remove();
    document.body.appendChild(this.root);
  }

  /** The active profile's secret, mirrored for the next turn. */
  activeApiKey(): string {
    return this.apiKey;
  }

  isOpen(): boolean {
    return this.opened;
  }

  open(settings: AgentSettings): void {
    this.settings = settings;
    this.render();
    if (!this.root.open) {
      if (typeof this.root.showModal === "function") this.root.showModal();
      else this.root.setAttribute("open", "");
    }
    this.opened = true;
    this.root.querySelector<HTMLElement>("[data-agent-focus='provider.name']")?.focus();
  }

  close(): void {
    if (this.root.open && typeof this.root.close === "function") this.root.close();
    else this.root.removeAttribute("open");
    this.opened = false;
  }

  /** Reload the active profile's credential from the credential store. */
  async refreshCredential(settings?: AgentSettings): Promise<void> {
    if (settings) this.settings = settings;
    const current = this.settings;
    if (!current) return;
    const profile = activeProfile(current);
    this.apiKey = "";
    this.credentialError = "";
    if (!this.desktopAvailable) {
      this.credential = "missing";
      if (this.opened) this.render();
      return;
    }
    this.credential = "loading";
    if (this.opened) this.render();
    try {
      this.apiKey = (await this.handlers.onCredentialGet(profile.id)) ?? "";
      this.credential = this.apiKey ? "saved" : "missing";
    } catch (error) {
      this.credential = "failed";
      this.credentialError = error instanceof Error ? error.message : String(error);
    }
    if (this.opened) this.render();
  }

  /** Rebuild the dialog contents from the current settings. */
  render(): void {
    const settings = this.settings;
    if (!settings) return;
    const focus = this.captureFocus();
    this.root.replaceChildren(
      this.renderHeader(),
      this.renderBody(settings),
      this.renderFooter(),
    );
    this.restoreFocus(focus);
  }

  private captureFocus(): FocusSnapshot | null {
    const active = document.activeElement;
    if (!(active instanceof HTMLElement) || !this.root.contains(active)) return null;
    const field = active.dataset.agentFocus;
    if (!field) return null;
    const editable = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
    return {
      field,
      start: editable ? active.selectionStart : null,
      end: editable ? active.selectionEnd : null,
    };
  }

  private restoreFocus(focus: FocusSnapshot | null): void {
    if (!focus) return;
    const target = this.root.querySelector<HTMLElement>(`[data-agent-focus="${focus.field}"]`);
    if (!target) return;
    target.focus();
    if (
      focus.start === null ||
      focus.end === null ||
      !(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
    ) {
      return;
    }
    try {
      target.setSelectionRange(focus.start, focus.end);
    } catch {
      // Number inputs do not expose a text selection range.
    }
  }

  private renderHeader(): HTMLElement {
    const header = document.createElement("div");
    header.className = "modal__header";
    const title = document.createElement("h2");
    title.className = "modal__title";
    title.textContent = t("agent.settingsTitle");
    const close = document.createElement("button");
    close.type = "button";
    close.className = "modal__close";
    close.textContent = "×";
    close.title = t("agent.close");
    close.setAttribute("aria-label", t("agent.close"));
    close.addEventListener("click", () => this.close());
    header.append(title, close);
    return header;
  }

  private renderBody(settings: AgentSettings): HTMLElement {
    const body = document.createElement("div");
    body.className = "modal__body agent-settings__body";
    body.append(
      this.renderProviderSection(settings),
      this.renderTemplateSection(settings),
      this.renderAdvancedSection(settings),
    );
    return body;
  }

  private renderFooter(): HTMLElement {
    const footer = document.createElement("div");
    footer.className = "modal__footer";
    const hint = document.createElement("span");
    hint.className = "muted agent-settings__status";
    hint.setAttribute("role", "status");
    hint.textContent = this.status;
    const close = document.createElement("button");
    close.type = "button";
    close.className = "btn";
    close.textContent = t("agent.close");
    close.addEventListener("click", () => this.close());
    footer.append(hint, close);
    return footer;
  }

  private section(titleKey: string): HTMLElement {
    const section = document.createElement("section");
    section.className = "agent-settings__section";
    const heading = document.createElement("h3");
    heading.className = "agent-settings__heading";
    heading.textContent = t(titleKey);
    section.appendChild(heading);
    return section;
  }

  private renderProviderSection(settings: AgentSettings): HTMLElement {
    const section = this.section("agent.providerSection");
    const profile = activeProfile(settings);

    const row = document.createElement("div");
    row.className = "agent-profile-row";
    const select = document.createElement("select");
    select.className = "input input--small";
    select.dataset.agentFocus = "provider-profile";
    select.setAttribute("aria-label", t("agent.profileChoose"));
    for (const item of settings.profiles) {
      const option = document.createElement("option");
      option.value = item.id;
      option.textContent = `${item.name} · ${item.model}`;
      select.appendChild(option);
    }
    select.value = profile.id;
    select.addEventListener("change", () => {
      settings.activeProfileId = select.value;
      this.apiKey = "";
      this.persist();
      void this.refreshCredential();
    });

    const add = this.action(t("agent.addProfile"), "provider-add-profile", () => {
      const next = newProfile(settings.profiles.length + 1);
      settings.profiles.push(next);
      settings.activeProfileId = next.id;
      this.apiKey = "";
      this.credential = "missing";
      this.persist();
      void this.refreshCredential();
    });
    const duplicate = this.action(t("agent.duplicateProfile"), "provider-duplicate-profile", () => {
      const copy: AgentProviderProfile = {
        ...profile,
        id: globalThis.crypto?.randomUUID?.() ?? `profile-${Date.now().toString(36)}`,
        name: `${profile.name} copy`,
      };
      settings.profiles.push(copy);
      settings.activeProfileId = copy.id;
      this.apiKey = "";
      this.persist();
      void this.refreshCredential();
    });
    const remove = this.removeAction(
      t("agent.removeProfile"),
      "provider-remove-profile",
      `profile:${profile.id}`,
      () => {
        const removed = profile.id;
        settings.profiles = settings.profiles.filter((item) => item.id !== removed);
        settings.activeProfileId = settings.profiles[0].id;
        this.apiKey = "";
        this.persist();
        void this.handlers.onCredentialDelete(removed).catch(() => undefined);
        void this.refreshCredential();
      },
    );
    remove.disabled = settings.profiles.length <= 1;
    row.append(select, add, duplicate, remove);
    section.appendChild(row);

    section.appendChild(this.renderPresetRow(settings, profile));

    const grid = document.createElement("div");
    grid.className = "agent-settings__grid";
    const fields: Array<[keyof AgentProviderProfile, string, string, string]> = [
      ["name", "agent.profileName", "text", profile.name],
      ["endpoint", "agent.endpoint", "text", profile.endpoint],
      ["model", "agent.model", "text", profile.model],
      ["timeoutMs", "agent.timeout", "number", String(profile.timeoutMs)],
    ];
    for (const [key, labelKey, type, value] of fields) {
      grid.appendChild(this.renderProfileField(settings, key, labelKey, type, value));
    }
    grid.appendChild(this.renderCredentialField(profile));
    section.appendChild(grid);

    for (const key of ["agent.apiKeyHint", "agent.visionRuntimeHint", "agent.ocrHint"]) {
      const hint = document.createElement("p");
      hint.className = "field__hint";
      hint.textContent = this.desktopAvailable || key !== "agent.apiKeyHint"
        ? t(key)
        : t("agent.credentialDesktopOnly");
      section.appendChild(hint);
    }
    return section;
  }

  private renderProfileField(
    settings: AgentSettings,
    key: keyof AgentProviderProfile,
    labelKey: string,
    type: string,
    value: string,
  ): HTMLElement {
    const label = document.createElement("label");
    label.className = "field";
    const id = `agent-profile-${String(key)}`;
    const title = document.createElement("span");
    title.className = "field__label";
    title.textContent = t(labelKey);
    title.id = `${id}-label`;
    const input = document.createElement("input");
    input.id = id;
    input.className = "input";
    input.type = type;
    input.value = value;
    input.dataset.agentFocus = `provider.${String(key)}`;
    if (type === "number") {
      input.min = String(TIMEOUT_MIN);
      input.max = String(TIMEOUT_MAX);
      input.step = "1000";
    }
    input.setAttribute("aria-labelledby", title.id);
    const error = document.createElement("span");
    error.className = "field__hint field__hint--error";
    error.hidden = true;
    const validate = (raw: string): string => {
      if (key === "name") return raw.trim() ? "" : t("agent.invalidProfileName");
      if (key === "endpoint") return isEndpoint(raw) ? "" : t("agent.invalidEndpoint");
      if (key === "model") return raw.trim() ? "" : t("agent.invalidModel");
      const timeout = Number(raw);
      return Number.isFinite(timeout) && timeout >= TIMEOUT_MIN && timeout <= TIMEOUT_MAX
        ? ""
        : t("agent.invalidTimeout");
    };
    input.addEventListener("input", () => {
      const message = validate(input.value);
      error.textContent = message;
      error.hidden = message === "";
      input.setAttribute("aria-invalid", String(message !== ""));
      // An invalid value is never written through: the previous good value stays
      // in effect instead of being silently coerced to a default.
      if (message) return;
      const target = settings.profiles.find((item) => item.id === settings.activeProfileId);
      if (!target) return;
      if (key === "timeoutMs") target.timeoutMs = Number(input.value);
      else if (key === "name" || key === "endpoint" || key === "model") target[key] = input.value;
      this.persist();
    });
    label.append(title, input, error);
    return label;
  }

  private renderPresetRow(settings: AgentSettings, profile: AgentProviderProfile): HTMLElement {
    const row = document.createElement("div");
    row.className = "agent-preset-row";
    const select = document.createElement("select");
    select.className = "input input--small";
    select.dataset.agentFocus = "provider-preset";
    select.setAttribute("aria-label", t("agent.presetPlaceholder"));
    const none = document.createElement("option");
    none.value = "";
    none.textContent = t("agent.presetPlaceholder");
    select.appendChild(none);
    for (const preset of PROVIDER_PRESETS) {
      const option = document.createElement("option");
      option.value = preset.id;
      option.textContent = t(preset.labelKey);
      select.appendChild(option);
    }
    const detected = matchingPreset(profile);
    select.value = detected?.id ?? "";
    const apply = this.action(t("agent.presetApply"), "provider-preset-apply", () => {
      const preset = PROVIDER_PRESETS.find((item) => item.id === select.value);
      if (!preset) return;
      const target = settings.profiles.find((item) => item.id === settings.activeProfileId);
      if (!target) return;
      target.endpoint = preset.endpoint;
      target.model = preset.model;
      this.flash(t("agent.presetApplied", { name: t(preset.labelKey) }));
      this.persist();
      this.render();
    });
    apply.disabled = select.value === "";
    select.addEventListener("change", () => { apply.disabled = select.value === ""; });
    row.append(select, apply);
    return row;
  }

  private renderCredentialField(profile: AgentProviderProfile): HTMLElement {
    const wrapper = document.createElement("div");
    wrapper.className = "field agent-credential";
    const title = document.createElement("span");
    title.className = "field__label";
    title.textContent = t("agent.apiKey");
    const id = "agent-profile-api-key";
    title.id = `${id}-label`;
    const input = document.createElement("input");
    input.id = id;
    input.className = "input";
    input.type = "password";
    input.autocomplete = "new-password";
    input.spellcheck = false;
    input.value = this.apiKey;
    input.disabled = !this.desktopAvailable;
    input.dataset.agentFocus = "provider.apiKey";
    input.setAttribute("aria-labelledby", title.id);
    input.addEventListener("change", () => {
      this.apiKey = input.value;
      this.credential = input.value ? "saved" : "missing";
      this.credentialError = "";
      void this.handlers.onCredentialSet(profile.id, input.value)
        .then(() => this.flash(t("agent.credentialSaved")))
        .catch((error) => {
          this.credential = "failed";
          this.credentialError = error instanceof Error ? error.message : String(error);
        })
        .finally(() => { if (this.opened) this.render(); });
    });
    const chip = document.createElement("span");
    chip.className = `agent-credential__state agent-credential__state--${this.credential}`;
    chip.dataset.agentFocus = "provider.apiKeyState";
    chip.textContent = this.credentialText();
    wrapper.append(title, input, chip);
    if (this.credential === "failed") {
      const retry = this.action(t("agent.credentialRetry"), "provider-api-key-retry", () => {
        void this.refreshCredential();
      });
      retry.className = "btn btn--small agent-credential__retry";
      retry.title = this.credentialError;
      wrapper.appendChild(retry);
    }
    return wrapper;
  }

  private credentialText(): string {
    switch (this.credential) {
      case "loading": return t("agent.credentialLoading");
      case "saved": return t("agent.credentialSaved");
      case "failed": return t("agent.credentialFailed");
      default: return t("agent.credentialMissing");
    }
  }

  private renderTemplateSection(settings: AgentSettings): HTMLElement {
    const section = this.section("agent.promptTemplates");
    const builtinHint = document.createElement("p");
    builtinHint.className = "field__hint";
    builtinHint.textContent = t("agent.builtinTemplateHint");
    section.appendChild(builtinHint);
    for (const template of builtinTemplates()) {
      section.appendChild(this.renderBuiltinTemplate(template));
    }

    const heading = document.createElement("div");
    heading.className = "agent-templates__heading";
    const title = document.createElement("strong");
    title.textContent = t("agent.myTemplates");
    const add = this.action(t("agent.addTemplate"), "template-add", () => {
      settings.templates.push(newTemplate(settings.templates.length + 1));
      this.persist();
      this.render();
    });
    heading.append(title, add);
    section.appendChild(heading);

    if (settings.templates.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("agent.noTemplates");
      section.appendChild(empty);
    }
    for (const template of settings.templates) {
      section.appendChild(this.renderUserTemplate(settings, template));
    }
    return section;
  }

  private renderBuiltinTemplate(template: AgentPromptTemplate): HTMLElement {
    const row = document.createElement("div");
    row.className = "agent-template agent-template--builtin";
    const head = document.createElement("div");
    head.className = "agent-template__head";
    const name = document.createElement("strong");
    name.textContent = template.name;
    const badge = document.createElement("span");
    badge.className = "agent-template__badge";
    badge.textContent = t("agent.builtinTemplate");
    head.append(name, badge);
    const instructions = document.createElement("p");
    instructions.className = "agent-template__instructions";
    instructions.textContent = template.instructions;
    row.append(head, instructions);
    return row;
  }

  private renderUserTemplate(settings: AgentSettings, template: AgentPromptTemplate): HTMLElement {
    const row = document.createElement("div");
    row.className = "agent-template";
    const name = document.createElement("input");
    name.className = "input input--small";
    name.value = template.name;
    name.setAttribute("aria-label", t("agent.profileName"));
    name.dataset.agentFocus = `template.name.${template.id}`;
    name.addEventListener("input", () => { template.name = name.value; this.persist(); });
    const instructions = document.createElement("textarea");
    instructions.className = "input input--code";
    instructions.rows = 3;
    instructions.placeholder = t("agent.templateInstructions");
    instructions.value = template.instructions;
    instructions.dataset.agentFocus = `template.instructions.${template.id}`;
    instructions.addEventListener("input", () => {
      template.instructions = instructions.value;
      this.persist();
      this.refreshComposition();
    });
    const remove = this.removeAction(
      t("actions.delete"),
      `template-remove.${template.id}`,
      `template:${template.id}`,
      () => {
        settings.templates = settings.templates.filter((item) => item.id !== template.id);
        this.persist();
        this.render();
      },
    );
    row.append(name, instructions, remove);
    return row;
  }

  private renderAdvancedSection(settings: AgentSettings): HTMLElement {
    const section = this.section("agent.extraSection");
    const label = document.createElement("label");
    label.className = "field";
    const title = document.createElement("span");
    title.className = "field__label";
    title.textContent = t("agent.extraInstructionsLabel");
    title.id = "agent-extra-label";
    const extra = document.createElement("textarea");
    extra.className = "input input--code agent-extra-instructions";
    extra.rows = 3;
    extra.placeholder = t("agent.extraInstructions");
    extra.value = settings.extraInstructions;
    extra.dataset.agentFocus = "extra-instructions";
    extra.setAttribute("aria-labelledby", title.id);
    extra.addEventListener("input", () => {
      settings.extraInstructions = extra.value;
      this.persist();
      this.refreshComposition();
    });
    label.append(title, extra);
    section.append(label, this.renderComposition(settings));
    return section;
  }

  private renderComposition(settings: AgentSettings): HTMLElement {
    const box = document.createElement("div");
    box.className = "agent-composition";
    box.dataset.agentFocus = "composition";
    const title = document.createElement("strong");
    title.textContent = t("agent.templatePreview");
    const list = document.createElement("ol");
    list.className = "agent-composition__list";
    const parts = settings.extraInstructions.trim() ? [settings.extraInstructions.trim()] : [];
    for (const part of parts) {
      const item = document.createElement("li");
      item.textContent = part;
      list.appendChild(item);
    }
    const note = document.createElement("p");
    note.className = "muted agent-composition__note";
    note.textContent = parts.length === 0
      ? t("agent.templatePreviewEmpty")
      : t("agent.templateChars", { count: parts.join(" ").length });
    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = t("agent.templatePreviewHint");
    box.append(title, list, note, hint);
    return box;
  }

  private refreshComposition(): void {
    const settings = this.settings;
    const existing = this.root.querySelector(".agent-composition");
    if (!settings || !existing) return;
    existing.replaceWith(this.renderComposition(settings));
  }

  /** Two-step destructive action: the first click only asks for confirmation. */
  private removeAction(
    label: string,
    focus: string,
    key: string,
    apply: () => void,
  ): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "btn btn--small";
    button.dataset.agentFocus = focus;
    const pending = this.confirmPending === key;
    button.classList.toggle("btn--danger", pending);
    button.textContent = pending ? t("agent.confirmRemove") : label;
    button.addEventListener("click", () => {
      if (this.confirmPending !== key) {
        this.confirmPending = key;
        button.classList.add("btn--danger");
        button.textContent = t("agent.confirmRemove");
        return;
      }
      this.confirmPending = null;
      apply();
    });
    return button;
  }

  private action(label: string, focus: string, apply: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "btn btn--small";
    button.dataset.agentFocus = focus;
    button.textContent = label;
    button.addEventListener("click", apply);
    return button;
  }

  private persist(): void {
    this.handlers.onChange();
  }

  /**
   * A short-lived message in the dialog footer.
   *
   * Deliberately not a timer: a message that stays until the next action is both
   * easier to test and easier to read than a toast that disappears.
   */
  private flash(message: string): void {
    this.status = message;
    const node = this.root.querySelector(".agent-settings__status");
    if (node) node.textContent = message;
  }
}
