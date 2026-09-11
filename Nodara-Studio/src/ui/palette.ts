/**
 * The node palette.
 *
 * Nothing here is hardcoded: the palette is built entirely from the descriptors
 * the runtime reports. Installing a new plugin changes this panel without the
 * Studio being rebuilt.
 */

import { NodeDescriptor } from "../runtime/types";

export interface PaletteHandlers {
  onAdd: (descriptor: NodeDescriptor) => void;
}

export class Palette {
  private readonly categories = new Map<string, NodeDescriptor[]>();

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: PaletteHandlers,
  ) {}

  setDescriptors(descriptors: NodeDescriptor[]): void {
    this.categories.clear();
    for (const descriptor of descriptors) {
      const bucket = this.categories.get(descriptor.category) ?? [];
      bucket.push(descriptor);
      this.categories.set(descriptor.category, bucket);
    }
    for (const bucket of this.categories.values()) {
      bucket.sort((a, b) => a.display_name.localeCompare(b.display_name));
    }
    this.render("");
  }

  /** Case-insensitive filter across node type, display name and description. */
  filter(query: string): void {
    this.render(query.trim().toLowerCase());
  }

  private render(query: string): void {
    this.root.replaceChildren();
    const groups = [...this.categories.entries()].sort(([a], [b]) => a.localeCompare(b));

    let matches = 0;
    for (const [category, descriptors] of groups) {
      const visible = descriptors.filter(
        (descriptor) =>
          query === "" ||
          descriptor.node_type.toLowerCase().includes(query) ||
          descriptor.display_name.toLowerCase().includes(query) ||
          descriptor.description.toLowerCase().includes(query),
      );
      if (visible.length === 0) continue;
      matches += visible.length;

      const heading = document.createElement("h3");
      heading.className = "palette__category";
      heading.textContent = category;
      this.root.appendChild(heading);

      for (const descriptor of visible) {
        const item = document.createElement("button");
        item.className = "palette__item";
        item.type = "button";
        item.draggable = true;
        item.dataset.nodeType = descriptor.node_type;
        item.title = `${descriptor.node_type}\n${descriptor.description}`;

        const name = document.createElement("span");
        name.className = "palette__name";
        name.textContent = descriptor.display_name;
        item.appendChild(name);

        if (descriptor.dangerous) {
          const badge = document.createElement("span");
          badge.className = "badge badge--warn";
          badge.textContent = "gated";
          badge.title = `Requires: ${descriptor.permissions.join(", ") || "approval"}`;
          item.appendChild(badge);
        }

        item.addEventListener("dblclick", () => this.handlers.onAdd(descriptor));
        item.addEventListener("dragstart", (event) => {
          event.dataTransfer?.setData("application/x-nodara-node-type", descriptor.node_type);
          event.dataTransfer?.setData("text/plain", descriptor.node_type);
          if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy";
        });
        this.root.appendChild(item);
      }
    }

    if (matches === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = this.categories.size === 0
        ? "No node types reported. Is the runtime running?"
        : "No node types match that filter.";
      this.root.appendChild(empty);
    }
  }
}
