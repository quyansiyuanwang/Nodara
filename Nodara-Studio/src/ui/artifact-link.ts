import { invoke, isTauri } from "@tauri-apps/api/core";
import { createElement, Download } from "lucide";
import { t } from "../i18n";
import type { ArtifactMeta } from "../runtime/types";

export function artifactMeta(value: unknown): ArtifactMeta | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Record<string, unknown>;
  if (typeof record.id !== "string" || !record.id || typeof record.content_type !== "string" || typeof record.size !== "number") return null;
  return {
    id: record.id,
    name: typeof record.name === "string" ? record.name : record.id,
    content_type: record.content_type,
    size: record.size,
  };
}

export function mailDownload(url: string, artifact: ArtifactMeta): HTMLAnchorElement {
  const link = document.createElement("a");
  link.className = "mail-download";
  link.href = url;
  link.download = artifact.name;
  link.title = artifact.name;
  link.setAttribute("aria-label", `${t("recipe.email")} · ${artifact.name}`);
  link.append(createElement(Download, { width: 14, height: 14, "aria-hidden": "true" }), t("recipe.email"));
  if (isTauri()) link.addEventListener("click", async (event) => {
    event.preventDefault();
    try {
      await invoke("open_artifact_url", { url });
    } catch {
      link.textContent = t("artifact.downloadRetry");
      link.setAttribute("aria-label", link.textContent);
    }
  });
  return link;
}
