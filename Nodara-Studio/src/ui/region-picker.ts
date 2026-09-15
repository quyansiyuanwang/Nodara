import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { t } from "../i18n";
import { ArtifactMeta, RunSnapshot, Workflow } from "../runtime/types";

export interface ScreenRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface RegionPickerClient {
  createRun(workflow: Workflow, variables?: Record<string, unknown>, startPaused?: boolean): Promise<RunSnapshot>;
  getRun(runId: string): Promise<RunSnapshot>;
  listArtifacts(runId: string): Promise<ArtifactMeta[]>;
  artifactUrl(runId: string, artifactId: string): string;
}

interface PixelPoint {
  x: number;
  y: number;
}

/** Capture a fresh desktop image and let the user drag a region over it. */
export class RegionPicker {
  private settled = false;
  private finish: ((region: ScreenRegion | null) => void) | null = null;

  constructor(
    private readonly root: HTMLDialogElement,
    private readonly client: RegionPickerClient,
  ) {
    this.root.addEventListener("close", () => this.complete(null));
  }

  async pick(): Promise<ScreenRegion | null> {
    this.settled = false;
    try {
      const imageUrl = await this.captureWithStudioHidden();
      if (this.settled) return null;
      this.open();
      return new Promise<ScreenRegion | null>((resolve) => {
        this.finish = resolve;
        this.renderImage(imageUrl);
      });
    } catch (error) {
      if (this.settled) return null;
      this.open();
      return new Promise<ScreenRegion | null>((resolve) => {
        this.finish = resolve;
        this.renderError((error as Error).message ?? String(error));
      });
    }
  }

  /**
   * Hide the desktop shell while the runtime captures the screen. Otherwise
   * the screenshot would contain Studio itself instead of the application the
   * user wants to select a region from.
   */
  private async captureWithStudioHidden(): Promise<string> {
    if (!isTauri()) return this.captureDesktop();
    const appWindow = getCurrentWindow();
    let hidden = false;
    try {
      await appWindow.hide();
      hidden = true;
      await delay(180);
    } catch {
      // Window permissions may be unavailable in a development shell. A
      // visible capture is still useful, so continue without hiding it.
    }
    try {
      return await this.captureDesktop();
    } finally {
      if (hidden) {
        try {
          await appWindow.show();
          await appWindow.setFocus();
        } catch {
          // The picker remains usable even if the shell cannot be restored.
        }
      }
    }
  }

  private open(): void {
    if (!this.root.open) {
      if (typeof this.root.showModal === "function") this.root.showModal();
      else this.root.setAttribute("open", "");
    }
  }

  private complete(region: ScreenRegion | null): void {
    if (this.settled) return;
    this.settled = true;
    const finish = this.finish;
    this.finish = null;
    if (this.root.open && typeof this.root.close === "function") this.root.close();
    else this.root.removeAttribute("open");
    finish?.(region);
  }

  private async captureDesktop(): Promise<string> {
    const workflow: Workflow = {
      schema_version: "2.0",
      id: "workflow.studio.region-picker",
      metadata: { name: "Studio region picker", tags: ["studio"] },
      nodes: [
        { id: "start", type: "core.Start", config: {} },
        {
          id: "capture",
          type: "windows.Desktop.Capture",
          config: { x: 0, y: 0, output_var: "picker_artifact" },
        },
        { id: "end", type: "core.End", config: {} },
      ],
      edges: [
        { id: "start-capture", source: "start", target: "capture" },
        { id: "capture-end", source: "capture", target: "end" },
      ],
      variables: {},
    };
    const run = await this.client.createRun(workflow);
    let snapshot = run;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      if (["completed", "failed", "cancelled"].includes(snapshot.status)) break;
      await new Promise((resolve) => window.setTimeout(resolve, 100));
      snapshot = await this.client.getRun(run.id);
    }
    if (snapshot.status !== "completed") {
      const detail = snapshot.failure
        ? `${snapshot.failure.code}: ${snapshot.failure.message}`
        : snapshot.status;
      throw new Error(t("capture.captureFailed", { detail }));
    }
    const artifacts = await this.client.listArtifacts(run.id);
    const artifact = artifacts.find((item) => item.content_type.startsWith("image/"));
    if (!artifact) throw new Error(t("capture.noImage"));
    return this.client.artifactUrl(run.id, artifact.id);
  }

  private renderError(message: string): void {
    this.root.replaceChildren();
    this.root.append(
      this.header(),
      this.message(message, "region-picker__status region-picker__status--error"),
      this.footer(() => this.complete(null)),
    );
  }

  private renderImage(imageUrl: string): void {
    this.root.replaceChildren();
    const body = document.createElement("div");
    body.className = "modal__body region-picker__body";
    const hint = document.createElement("p");
    hint.className = "modal__hint";
    hint.textContent = t("capture.dragHint");
    const stage = document.createElement("div");
    stage.className = "region-picker__stage";
    const image = document.createElement("img");
    image.className = "region-picker__image";
    image.src = imageUrl;
    image.alt = t("capture.sourceAlt");
    const selectionBox = document.createElement("div");
    selectionBox.className = "region-picker__selection";
    selectionBox.hidden = true;
    const readout = document.createElement("p");
    readout.className = "region-picker__readout";
    readout.textContent = t("capture.noSelection");
    stage.append(image, selectionBox);
    body.append(hint, stage, readout);

    let dragging = false;
    let start: PixelPoint | null = null;
    let selection: ScreenRegion | null = null;
    let apply: HTMLButtonElement | null = null;
    const renderedPoint = (event: PointerEvent): PixelPoint => {
      const rect = image.getBoundingClientRect();
      return {
        x: Math.max(0, Math.min(rect.width, event.clientX - rect.left)),
        y: Math.max(0, Math.min(rect.height, event.clientY - rect.top)),
      };
    };
    const update = (current: PixelPoint) => {
      if (!start) return;
      const width = image.naturalWidth || image.getBoundingClientRect().width;
      const height = image.naturalHeight || image.getBoundingClientRect().height;
      const renderedWidth = image.getBoundingClientRect().width || width;
      const renderedHeight = image.getBoundingClientRect().height || height;
      const left = Math.min(start.x, current.x);
      const top = Math.min(start.y, current.y);
      const right = Math.max(start.x, current.x);
      const bottom = Math.max(start.y, current.y);
      selectionBox.hidden = false;
      selectionBox.style.left = `${left}px`;
      selectionBox.style.top = `${top}px`;
      selectionBox.style.width = `${right - left}px`;
      selectionBox.style.height = `${bottom - top}px`;
      selection = {
        x: Math.round((left / renderedWidth) * width),
        y: Math.round((top / renderedHeight) * height),
        width: Math.max(1, Math.round(((right - left) / renderedWidth) * width)),
        height: Math.max(1, Math.round(((bottom - top) / renderedHeight) * height)),
      };
      readout.textContent = t("capture.selection", {
        x: selection.x,
        y: selection.y,
        width: selection.width,
        height: selection.height,
      });
      if (apply) apply.disabled = selection.width < 2 || selection.height < 2;
    };
    image.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      dragging = true;
      image.setPointerCapture(event.pointerId);
      start = renderedPoint(event);
      update(start);
    });
    image.addEventListener("pointermove", (event) => {
      if (dragging) update(renderedPoint(event));
    });
    image.addEventListener("pointerup", (event) => {
      if (!dragging) return;
      dragging = false;
      update(renderedPoint(event));
    });
    image.addEventListener("pointercancel", () => {
      dragging = false;
    });

    const footer = this.footer(() => this.complete(null));
    const applyButton = document.createElement("button");
    apply = applyButton;
    applyButton.type = "button";
    applyButton.className = "btn btn--primary";
    applyButton.textContent = t("capture.applyRegion");
    applyButton.disabled = true;
    applyButton.addEventListener("click", () => this.complete(selection));
    footer.appendChild(applyButton);
    this.root.append(this.header(), body, footer);
  }

  private header(): HTMLElement {
    const header = document.createElement("div");
    header.className = "modal__header";
    const title = document.createElement("h2");
    title.className = "modal__title";
    title.textContent = t("capture.selectRegion");
    const close = document.createElement("button");
    close.type = "button";
    close.className = "modal__close";
    close.textContent = "×";
    close.title = t("runDialog.close");
    close.setAttribute("aria-label", t("runDialog.close"));
    close.addEventListener("click", () => this.complete(null));
    header.append(title, close);
    return header;
  }

  private message(text: string, className: string): HTMLElement {
    const body = document.createElement("div");
    body.className = "modal__body";
    const message = document.createElement("p");
    message.className = className;
    message.textContent = text;
    body.appendChild(message);
    return body;
  }

  private footer(cancel: () => void): HTMLElement {
    const footer = document.createElement("div");
    footer.className = "modal__footer";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "btn";
    button.textContent = t("actions.cancel");
    button.addEventListener("click", cancel);
    footer.appendChild(button);
    return footer;
  }
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}
