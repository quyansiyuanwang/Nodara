/**
 * i18n catalog guards.
 *
 * A message key that exists in only one language renders as a raw `agent.foo`
 * in the other one. That is exactly how `agent.profileName` reached the screen
 * in both languages, and how thirty `agent.*` strings appeared as keys in the
 * English build only. These tests read every module and the static HTML as text
 * and prove:
 *
 * 1. every catalog is complete for both languages;
 * 2. every key the code asks for exists in both;
 * 3. no key is defined in one language and missing in the other.
 *
 * Sources are read through Vite (`?raw` / `import.meta.glob`) rather than
 * `node:fs`, because the Studio's `tsconfig` deliberately has no Node types.
 */

import { describe, expect, it } from "vitest";

import pageSource from "../index.html?raw";
import { CATALOGS, hasMessage, Locale } from "./i18n";

const LOCALES: Locale[] = ["en", "zh-CN"];

const sources = import.meta.glob("./**/*.ts", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Prefixes that only ever appear as Studio message keys. */
function messageDomains(): Set<string> {
  const domains = new Set<string>();
  for (const catalog of Object.values(CATALOGS)) {
    for (const key of Object.keys(catalog)) domains.add(key.split(".")[0]);
  }
  return domains;
}

/**
 * Literals the UI can look up.
 *
 * `t("a.b")` calls are unambiguous. Dotted literals outside a call (the label
 * tables in the Agent panel pass `"agent.profileName"` around as data) are only
 * counted when their first segment is a known message domain, so node types
 * such as `windows.Input.Mouse`, storage keys such as `studio.visuals` and CSS
 * selectors stay out of the check.
 */
function referencedKeys(text: string, domains: Set<string>): Set<string> {
  const found = new Set<string>();
  for (const match of text.matchAll(/\bt\(\s*"([^"]+)"/g)) found.add(match[1]);
  for (const match of text.matchAll(/"([A-Za-z][A-Za-z0-9]*(?:\.[A-Za-z0-9_-]+)+)"/g)) {
    if (domains.has(match[1].split(".")[0])) found.add(match[1]);
  }
  return found;
}

function moduleKeys(): Map<string, string[]> {
  const domains = messageDomains();
  const used = new Map<string, string[]>();
  for (const [path, text] of Object.entries(sources)) {
    if (path.includes(".test.")) continue;
    for (const key of referencedKeys(text, domains)) {
      used.set(key, [...(used.get(key) ?? []), path]);
    }
  }
  return used;
}

function htmlKeys(): string[] {
  const keys: string[] = [];
  const pattern = /data-i18n(?:-placeholder|-title|-aria-label)?="([^"]+)"/g;
  for (const match of pageSource.matchAll(pattern)) keys.push(match[1]);
  return keys;
}

describe("i18n catalogs", () => {
  it("covers the same message keys in every language", () => {
    const english = Object.keys(CATALOGS.en).sort();
    const chinese = Object.keys(CATALOGS["zh-CN"]).sort();
    const missingInChinese = english.filter((key) => !hasMessage(key, "zh-CN"));
    const missingInEnglish = chinese.filter((key) => !hasMessage(key, "en"));
    expect({ missingInChinese, missingInEnglish }).toEqual({
      missingInChinese: [],
      missingInEnglish: [],
    });
  });

  it("defines every key the modules ask for in both languages", () => {
    const used = moduleKeys();
    // Guards the scan itself: a broken glob or regex would report nothing.
    expect(used.size).toBeGreaterThan(200);
    const missing: string[] = [];
    for (const [key, paths] of used) {
      for (const locale of LOCALES) {
        if (!hasMessage(key, locale)) missing.push(`${key} (${locale}, used by ${paths.join(", ")})`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("defines every key the static HTML asks for in both languages", () => {
    const keys = htmlKeys();
    expect(keys.length).toBeGreaterThan(10);
    const missing: string[] = [];
    for (const key of keys) {
      for (const locale of LOCALES) {
        if (!hasMessage(key, locale)) missing.push(`${key} (${locale})`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("never leaves a placeholder unfilled in either language", () => {
    const mismatched: string[] = [];
    for (const key of Object.keys(CATALOGS.en)) {
      const placeholders = (text: string) =>
        [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort().join(",");
      const english = placeholders(CATALOGS.en[key]);
      const chinese = placeholders(CATALOGS["zh-CN"][key] ?? "");
      if (english !== chinese) mismatched.push(`${key}: en[${english}] zh[${chinese}]`);
    }
    expect(mismatched).toEqual([]);
  });
});
