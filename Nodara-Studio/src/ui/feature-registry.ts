export type FeatureSource = "builtin" | "host" | "plugin";

export interface DrawerFeature {
  id: string;
  labelKey: string;
  panelId: string;
  order?: number;
  source?: FeatureSource;
}

/** Registry for Studio drawer features. Built-ins and future host/plugin UI use one path. */
export class FeatureRegistry {
  private readonly entries = new Map<string, DrawerFeature>();

  registerPanel(feature: DrawerFeature): this {
    this.entries.set(feature.id, {
      order: 0,
      source: "builtin",
      ...feature,
    });
    return this;
  }

  getPanel(id: string): DrawerFeature | undefined {
    return this.entries.get(id);
  }

  panels(): DrawerFeature[] {
    return [...this.entries.values()].sort(
      (left, right) =>
        (left.order ?? 0) - (right.order ?? 0) || left.id.localeCompare(right.id),
    );
  }

  get size(): number {
    return this.entries.size;
  }
}
