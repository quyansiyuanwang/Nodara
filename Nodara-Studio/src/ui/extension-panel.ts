import { localizeExtensionKind, t } from "../i18n";
import { ExtensionDescriptor } from "../runtime/types";

export class ExtensionPanel {
  constructor(private readonly root: HTMLElement) {}

  setExtensions(extensions: ExtensionDescriptor[]): void {
    this.root.replaceChildren();
    if (extensions.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = t("extensions.empty");
      this.root.appendChild(empty);
      return;
    }

    const table = document.createElement("table");
    table.className = "extensions__table";
    const head = document.createElement("thead");
    const row = document.createElement("tr");
    for (const key of [
      "extensions.name",
      "extensions.kind",
      "extensions.source",
      "extensions.capabilities",
      "extensions.nodes",
      "extensions.status",
    ]) {
      const cell = document.createElement("th");
      cell.textContent = t(key);
      row.appendChild(cell);
    }
    head.appendChild(row);
    table.appendChild(head);

    const body = document.createElement("tbody");
    for (const extension of extensions) {
      const row = document.createElement("tr");
      row.dataset.extensionId = extension.id;
      row.className = `extension-row extension-row--${extension.kind.replace(/_/g, "-")}`;
      const name = document.createElement("td");
      const title = document.createElement("strong");
      title.textContent = extension.name;
      const id = document.createElement("small");
      id.textContent = extension.id;
      name.append(title, id);
      const kind = document.createElement("td");
      kind.textContent = localizeExtensionKind(extension.kind);
      const source = document.createElement("td");
      source.textContent = extension.source;
      const capabilities = document.createElement("td");
      capabilities.textContent = String(extension.capabilities.length);
      capabilities.title = extension.capabilities.join(", ");
      const nodes = document.createElement("td");
      nodes.textContent = String(extension.node_types.length);
      nodes.title = extension.node_types.join(", ");
      const status = document.createElement("td");
      status.textContent = extension.loaded ? t("extensions.loaded") : t("extensions.discovered");
      status.className = extension.loaded ? "extension-status--loaded" : "extension-status--pending";
      row.append(name, kind, source, capabilities, nodes, status);
      body.appendChild(row);
    }
    table.appendChild(body);
    this.root.appendChild(table);
  }
}
