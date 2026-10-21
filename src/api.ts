// 与 Rust 端命令对应的类型与调用封装。
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/** 是否运行在 Tauri 窗口里；在普通浏览器里打开时使用示例数据（仅用于预览界面）。 */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return tauriInvoke<T>(cmd, args);
  const { mockInvoke } = await import("./mock");
  return (await mockInvoke(cmd, args)) as T;
}

export type Source = "bmclapi" | "official";
export type Category = "release" | "snapshot" | "old" | "aprilFools";
export type LoaderKind = "forge" | "neoForge" | "fabric" | "fabricApi" | "quilt" | "optiFine";

export interface Settings {
  onboarded: boolean;
  gameDirs: string[];
  currentDir: string | null;
  source: Source;
  concurrency: number;
  isolate: boolean;
  language: string;
  autoJava: boolean;
  javaPaths: string[];
}

export interface DirInfo {
  path: string;
  source: string;
  exists: boolean;
  writable: boolean;
  isPcl: boolean;
  versionCount: number;
  message: string;
}

export interface JavaInfo {
  path: string;
  major: number;
  version: string;
  is64bit: boolean;
  source: string;
}

export interface VersionEntry {
  id: string;
  category: Category;
  kind: string;
  url: string;
  sha1: string | null;
  releaseTime: string;
}

export interface VersionList {
  latestRelease: string | null;
  latestSnapshot: string | null;
  versions: VersionEntry[];
}

export interface LoaderVersion {
  id: string;
  version: string;
  date: string | null;
  stable: boolean;
  tag: string | null;
  note: string | null;
  url: string | null;
  sha1: string | null;
}

export interface FabricApiPick {
  fileName: string;
  url: string;
  sha1: string | null;
  version: string;
}

export interface InstallRequest {
  gameDir: string;
  mcVersion: string;
  name: string;
  forge: string | null;
  neoforge: string | null;
  fabric: string | null;
  fabricApi: FabricApiPick | null;
  quilt: string | null;
  optifine: string | null;
  isolated: boolean;
  language: string | null;
  javaPath: string | null;
  autoJava: boolean;
}

export type StepState = "waiting" | "running" | "done" | "failed";
export type TaskStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export interface StepSnapshot {
  key: string;
  title: string;
  state: StepState;
  progress: number;
  detail: string;
}

export interface TaskSnapshot {
  id: number;
  kind: string;
  title: string;
  versionName: string;
  status: TaskStatus;
  steps: StepSnapshot[];
  progress: number;
  speed: number;
  remainingFiles: number;
  error: string | null;
  logs: string[];
}

export interface InstalledVersion {
  name: string;
  mcVersion: string;
  loaders: string[];
  isolated: boolean;
  problem: string | null;
  modified: number | null;
}

export interface VerifyReport {
  total: number;
  missing: number;
  unrecoverable: number;
  examples: string[];
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  detectGameDirs: () => invoke<DirInfo[]>("detect_game_dirs"),
  inspectGameDir: (path: string) => invoke<DirInfo>("inspect_game_dir", { path }),
  scanJava: (force: boolean) => invoke<JavaInfo[]>("scan_java", { force }),
  probeJava: (path: string) => invoke<JavaInfo>("probe_java", { path }),
  getVersionList: (force: boolean) => invoke<VersionList>("get_version_list", { force }),
  listLoader: (kind: LoaderKind, mc: string) => invoke<LoaderVersion[]>("list_loader", { kind, mc }),
  validateName: (dir: string, name: string) => invoke<string | null>("validate_name", { dir, name }),
  startInstall: (req: InstallRequest) => invoke<number>("start_install", { req }),
  startRepair: (dir: string, name: string) => invoke<number>("start_repair", { dir, name }),
  getTasks: () => invoke<TaskSnapshot[]>("get_tasks"),
  cancelTask: (id: number) => invoke<void>("cancel_task", { id }),
  dismissTask: (id: number) => invoke<void>("dismiss_task", { id }),
  listInstalled: (dir: string) => invoke<InstalledVersion[]>("list_installed", { dir }),
  verifyVersion: (dir: string, name: string) => invoke<VerifyReport>("verify_version", { dir, name }),
  deleteVersion: (dir: string, name: string) => invoke<void>("delete_version", { dir, name }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  runDirOf: (dir: string, name: string, isolated: boolean) => invoke<string>("run_dir_of", { dir, name, isolated }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
