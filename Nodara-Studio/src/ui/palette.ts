/**
 * The node palette.
 *
 * Nothing here is hardcoded: the palette is built entirely from the descriptors
 * the runtime reports. Installing a new plugin changes this panel without the
 * Studio being rebuilt.
 */

import { localizeCategory, localizeProblem, t } from "../i18n";
import { NodeTypeAdmission } from "../model/workflow";
import { NodeDescriptor } from "../runtime/types";

export interface PaletteHandlers {
  onAdd: (descriptor: NodeDescriptor) => void;
  /** Begin a pointer drag; the canvas owns the drop interaction. */
  onDragStart?: (descriptor: NodeDescriptor, event: PointerEvent) => void;
  /** Editor-level rules such as the single core.Start restriction. */
  allowed?: (descriptor: NodeDescriptor) => NodeTypeAdmission;
}

export class Palette {
  private readonly categories = new Map<string, NodeDescriptor[]>();
  /** The active filter; kept so a background refresh does not reset it. */
  private query = "";
  /** Suppresses the click generated after a pointer drag. */
  private suppressClick = false;
  private availabilityKey = "";

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
    this.render(this.query);
  }

  /** Re-render only when an item's add/disabled state actually changes. */
  refreshAvailability(): void {
    if (this.currentAvailabilityKey() === this.availabilityKey) return;
    this.render(this.query);
  }

  /** Case-insensitive filter across node type, display name and description. */
  filter(query: string): void {
    this.query = query.trim().toLowerCase();
    this.render(this.query);
  }

  private render(query: string): void {
    this.root.replaceChildren();
    this.availabilityKey = this.currentAvailabilityKey();
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

      const group = document.createElement("details");
      group.className = "palette__group";
      const storageKey = `nodara.palette.${category}.open`;
      let open = category.toLowerCase() === "core";
      try {
        const stored = localStorage.getItem(storageKey);
        if (stored !== null) open = stored === "true";
      } catch {
        // Palette state is optional in hardened webviews.
      }
      group.open = query !== "" || open;
      group.addEventListener("toggle", () => {
        try {
          localStorage.setItem(storageKey, String(group.open));
        } catch {
          // Keep working without storage.
        }
      });

      const heading = document.createElement("summary");
      heading.className = "palette__category";
      heading.textContent = localizeCategory(category);
      heading.dataset.count = String(visible.length);
      group.appendChild(heading);

      const items = document.createElement("div");
      items.className = "palette__items";

      for (const descriptor of visible) {
        const admission = this.handlers.allowed?.(descriptor) ?? { allowed: true };
        const item = document.createElement("button");
        item.className = "palette__item";
        item.type = "button";
        item.draggable = false;
        item.disabled = !admission.allowed;
        item.dataset.nodeType = descriptor.node_type;
        item.title = admission.allowed
          ? `${descriptor.node_type}\n${descriptor.description}\n\n${t("palette.addHint")}`
          : `${descriptor.node_type}\n${descriptor.description}\n\n${localizeProblem(admission.reason ?? "")}`;

        const name = document.createElement("span");
        name.className = "palette__name";
        name.textContent = descriptor.display_name;
        item.appendChild(name);

        if (descriptor.dangerous) {
          const badge = document.createElement("span");
          badge.className = "badge badge--warn";
          badge.textContent = t("palette.gated");
          badge.title = t("palette.requires", { permissions: descriptor.permissions.join(", ") || "approval" });
          item.appendChild(badge);
        }

        item.addEventListener("click", (event) => {
          if (item.disabled) return;
          if (this.suppressClick) {
            this.suppressClick = false;
            event.preventDefault();
            return;
          }
          this.handlers.onAdd(descriptor);
        });
        item.addEventListener("pointerdown", (event) => {
          if (item.disabled || event.button !== 0 || !this.handlers.onDragStart) return;
          this.suppressClick = false;
          const startX = event.clientX;
          const startY = event.clientY;
          const onMove = (moveEvent: PointerEvent) => {
            if (Math.hypot(moveEvent.clientX - startX, moveEvent.clientY - startY) >= 5) {
              this.suppressClick = true;
            }
          };
          const onEnd = () => {
            window.removeEventListener("pointermove", onMove);
            window.removeEventListener("pointerup", onEnd);
            window.removeEventListener("pointercancel", onEnd);
          };
          window.addEventListener("pointermove", onMove);
          window.addEventListener("pointerup", onEnd);
          window.addEventListener("pointercancel", onEnd);
          this.handlers.onDragStart(descriptor, event);
        });
        items.appendChild(item);
      }
      group.appendChild(items);
      this.root.appendChild(group);
    }

    if (matches === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = this.categories.size === 0
        ? t("palette.emptyRuntime")
        : t("palette.emptyFilter");
      this.root.appendChild(empty);
    }
  }

  private currentAvailabilityKey(): string {
    return [...this.categories.values()]
      .flat()
      .map((descriptor) => {
        const admission = this.handlers.allowed?.(descriptor) ?? { allowed: true };
        return `${descriptor.node_type}:${admission.allowed}`;
      })
      .join("|");
  }
}
