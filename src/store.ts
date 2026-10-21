// 全局状态：设置、版本列表、任务、Java 列表、页面导航、提示。
import { reactive } from "vue";
import {
  api,
  inTauri,
  errorText,
  type JavaInfo,
  type LoaderKind,
  type LoaderVersion,
  type Settings,
  type TaskSnapshot,
  type VersionEntry,
  type VersionList,
} from "./api";

export type Page = "versions" | "install" | "tasks" | "installed" | "settings";

export const store = reactive({
  ready: false,
  settings: null as Settings | null,
  page: "versions" as Page,
  versionList: null as VersionList | null,
  versionError: "",
  versionLoading: false,
  installTarget: null as VersionEntry | null,
  tasks: [] as TaskSnapshot[],
  java: [] as JavaInfo[],
  javaLoading: false,
  toast: { text: "", kind: "info" as "info" | "ok" | "bad", seq: 0 },
});

export function go(page: Page) {
  store.page = page;
}

let toastTimer: number | undefined;
export function toast(text: string, kind: "info" | "ok" | "bad" = "info") {
  store.toast = { text, kind, seq: store.toast.seq + 1 };
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (store.toast.text = ""), kind === "bad" ? 6000 : 3000);
}

export async function init() {
  store.settings = await api.getSettings();
  if (!inTauri) (await import("./mock")).previewSetup(store);
  store.ready = true;
  pollTasks();
  if (store.settings.onboarded) {
    loadVersions(false);
    refreshJava(false);
  }
}

export async function saveSettings(patch: Partial<Settings>) {
  if (!store.settings) return;
  const next = { ...store.settings, ...patch };
  try {
    await api.saveSettings(next);
    const sourceChanged = next.source !== store.settings.source;
    store.settings = next;
    if (sourceChanged) loaderCache.clear();
  } catch (e) {
    toast(`保存设置失败：${errorText(e)}`, "bad");
  }
}

/** 选中一个游戏目录（同时记到目录列表里）。 */
export async function useGameDir(path: string) {
  if (!store.settings) return;
  const dirs = store.settings.gameDirs.filter((d) => d.toLowerCase() !== path.toLowerCase());
  dirs.unshift(path);
  await saveSettings({ currentDir: path, gameDirs: dirs });
}

export async function loadVersions(force: boolean) {
  store.versionLoading = true;
  store.versionError = "";
  try {
    store.versionList = await api.getVersionList(force);
  } catch (e) {
    store.versionError = errorText(e);
  } finally {
    store.versionLoading = false;
  }
}

export async function refreshJava(force: boolean) {
  store.javaLoading = true;
  try {
    store.java = await api.scanJava(force);
  } catch (e) {
    toast(`扫描 Java 失败：${errorText(e)}`, "bad");
  } finally {
    store.javaLoading = false;
  }
}

const loaderCache = new Map<string, Promise<LoaderVersion[]>>();
export function loaderList(kind: LoaderKind, mc: string): Promise<LoaderVersion[]> {
  const key = `${kind}:${mc}`;
  let p = loaderCache.get(key);
  if (!p) {
    p = api.listLoader(kind, mc);
    p.catch(() => loaderCache.delete(key));
    loaderCache.set(key, p);
  }
  return p;
}

let pollTimer: number | undefined;
const finished = new Set<number>();
export async function pollTasks() {
  window.clearTimeout(pollTimer);
  try {
    const tasks = await api.getTasks();
    for (const t of tasks) {
      if ((t.status === "done" || t.status === "failed") && !finished.has(t.id)) {
        finished.add(t.id);
        if (t.status === "done") toast(`${t.versionName} ${t.kind === "repair" ? "补全完成" : "安装完成"}`, "ok");
        else toast(`${t.versionName} ${t.kind === "repair" ? "补全失败" : "安装失败"}`, "bad");
      }
    }
    store.tasks = tasks;
  } catch {
    /* 下次再试 */
  }
  const active = store.tasks.some((t) => t.status === "queued" || t.status === "running");
  pollTimer = window.setTimeout(pollTasks, active ? 500 : 2000);
}

export function activeTaskCount(): number {
  return store.tasks.filter((t) => t.status === "queued" || t.status === "running").length;
}
