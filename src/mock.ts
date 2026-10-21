// 浏览器预览用的示例数据：不在 Tauri 窗口里运行时代替 Rust 端命令，方便调界面。
// 用地址栏的 #页面名 切换初始页面，例如 #install、#tasks、#installed、#settings、#onboard。
import type { DirInfo, InstalledVersion, JavaInfo, LoaderVersion, Settings, TaskSnapshot, VersionList } from "./api";
import type { store as Store } from "./store";

const hash = location.hash.replace("#", "");

const settings: Settings = {
  onboarded: hash !== "onboard",
  gameDirs: ["D:\\Games\\Minecraft\\.minecraft", "C:\\Users\\Steve\\AppData\\Roaming\\.minecraft"],
  currentDir: "D:\\Games\\Minecraft\\.minecraft",
  source: "bmclapi",
  concurrency: 48,
  isolate: true,
  language: "zh_cn",
  autoJava: true,
  javaPaths: [],
};

const rel = (id: string, t: string) => ({ id, category: "release" as const, kind: "release", url: "", sha1: null, releaseTime: t });
const versions: VersionList = {
  latestRelease: "26.3",
  latestSnapshot: "26.4-snapshot-5",
  versions: [
    { id: "26.4-snapshot-5", category: "snapshot", kind: "snapshot", url: "", sha1: null, releaseTime: "2026-10-14T09:31:00+00:00" },
    rel("26.3", "2026-09-29T10:00:00+00:00"),
    rel("26.2", "2026-06-16T10:00:00+00:00"),
    rel("26.1", "2026-03-24T10:00:00+00:00"),
    rel("1.21.10", "2025-10-07T09:19:00+00:00"),
    rel("1.21.1", "2024-08-08T12:24:00+00:00"),
    rel("1.20.1", "2023-06-12T13:25:00+00:00"),
    rel("1.16.5", "2021-01-14T16:05:00+00:00"),
    rel("1.12.2", "2017-09-18T08:39:00+00:00"),
    rel("1.7.10", "2014-06-26T10:52:00+00:00"),
    { id: "26.4-snapshot-4", category: "snapshot", kind: "snapshot", url: "", sha1: null, releaseTime: "2026-10-07T09:31:00+00:00" },
    { id: "b1.7.3", category: "old", kind: "old_beta", url: "", sha1: null, releaseTime: "2011-07-07T22:00:00+00:00" },
    { id: "25w14craftmine", category: "aprilFools", kind: "snapshot", url: "", sha1: null, releaseTime: "2025-04-01T11:00:00+00:00" },
  ],
};

const lv = (id: string, version: string, extra: Partial<LoaderVersion> = {}): LoaderVersion => ({
  id,
  version,
  date: null,
  stable: true,
  tag: null,
  note: null,
  url: "https://example.invalid/x.jar",
  sha1: null,
  ...extra,
});
const loaders: Record<string, LoaderVersion[]> = {
  forge: [
    lv("1.20.1-47.4.28", "47.4.28", { tag: "最新", date: "2026-10-13T09:30:16Z" }),
    lv("1.20.1-47.4.27", "47.4.27", { date: "2026-10-12T18:57:17Z" }),
    lv("1.20.1-47.4.0", "47.4.0", { tag: "推荐", date: "2025-03-05T08:00:00Z" }),
    lv("1.20.1-47.3.0", "47.3.0", { date: "2024-06-12T08:00:00Z" }),
  ],
  neoForge: [lv("forge:1.20.1-47.1.106", "47.1.106", { tag: "最新" }), lv("forge:1.20.1-47.1.105", "47.1.105")],
  fabric: [lv("0.20.0", "0.20.0", { tag: "最新稳定版" }), lv("0.19.5", "0.19.5")],
  fabricApi: [lv("fabric-api-0.92.12+1.20.1.jar", "0.92.12+1.20.1", { tag: "最新", date: "2026-09-01T10:05:43Z" })],
  quilt: [lv("0.31.0-beta.4", "0.31.0-beta.4", { stable: false, tag: "最新" }), lv("0.29.1", "0.29.1", { tag: "推荐" })],
  optiFine: [
    lv("HD_U/I6", "HD_U_I6", { tag: "推荐", note: "兼容 Forge 47.2.18" }),
    lv("HD_U/I5", "HD_U_I5", { note: "兼容 Forge 47.0.35" }),
  ],
};

const java: JavaInfo[] = [
  { path: "C:\\Users\\Steve\\AppData\\Roaming\\.minecraft\\runtime\\java-runtime-delta\\bin\\java.exe", major: 21, version: "21.0.7", is64bit: true, source: "Minecraft 运行时" },
  { path: "C:\\Program Files\\Java\\jre1.8.0_431\\bin\\java.exe", major: 8, version: "1.8.0_431", is64bit: true, source: "系统安装" },
];

const dirs: DirInfo[] = [
  { path: "D:\\Games\\Minecraft\\.minecraft", source: "本程序所在目录", exists: true, writable: true, isPcl: false, versionCount: 6, message: "已有的游戏目录，6 个版本" },
  { path: "C:\\Users\\Steve\\AppData\\Roaming\\.minecraft", source: "官方启动器默认目录", exists: true, writable: true, isPcl: false, versionCount: 2, message: "已有的游戏目录，2 个版本" },
];

const installed: InstalledVersion[] = [
  { name: "1.20.1-Forge_47.4.0", mcVersion: "1.20.1", loaders: ["Forge 47.4.0"], isolated: true, problem: null, modified: 3 },
  { name: "1.21.1-Fabric 0.17.2", mcVersion: "1.21.1", loaders: ["Fabric 0.17.2"], isolated: true, problem: null, modified: 2 },
  { name: "1.20.1-OptiFine_HD_U_I6", mcVersion: "1.20.1", loaders: ["OptiFine HD_U_I6"], isolated: false, problem: null, modified: 1 },
  { name: "1.12.2", mcVersion: "1.12.2", loaders: [], isolated: false, problem: null, modified: 0 },
];

const step = (key: string, title: string, state: TaskSnapshot["steps"][number]["state"], progress: number, detail = "") => ({ key, title, state, progress, detail });
const tasks: TaskSnapshot[] =
  hash === "tasks"
    ? [
        {
          id: 1,
          kind: "install",
          title: "Minecraft 安装：1.21.1-Fabric 0.17.2",
          versionName: "1.21.1-Fabric 0.17.2",
          status: "done",
          steps: [step("meta", "获取版本信息", "done", 1, "Minecraft 1.21.1"), step("config", "写入配置", "done", 1, "完成")],
          progress: 1,
          speed: 0,
          remainingFiles: 0,
          error: null,
          logs: ["使用本机 Java 21.0.7"],
        },
        {
          id: 2,
          kind: "install",
          title: "Minecraft 安装：1.20.1-Forge_47.4.0",
          versionName: "1.20.1-Forge_47.4.0",
          status: "running",
          steps: [
            step("meta", "获取版本信息", "done", 1, "Minecraft 1.20.1"),
            step("client", "下载游戏本体", "done", 1, "已校验"),
            step("libraries", "下载依赖库", "done", 1, "127 个文件"),
            step("assets", "下载资源文件", "running", 0.34, "1289 / 3790"),
            step("java", "准备 Java", "running", 0.72, "下载 Java 17"),
            step("loader", "安装 Forge", "waiting", 0),
            step("config", "写入配置", "waiting", 0),
          ],
          progress: 0.632,
          speed: 12.4 * 1024 * 1024,
          remainingFiles: 2806,
          error: null,
          logs: [],
        },
      ]
    : [];

export async function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 120));
  switch (cmd) {
    case "get_settings":
      return settings;
    case "save_settings":
      Object.assign(settings, args.settings);
      return null;
    case "detect_game_dirs":
      return dirs;
    case "inspect_game_dir":
      return { path: args.path, source: "手动选择", exists: true, writable: true, isPcl: false, versionCount: 0, message: "空文件夹，可以直接使用" };
    case "scan_java":
      return java;
    case "probe_java":
      throw "找不到 java.exe，请选择 Java 安装目录里 bin 文件夹下的 java.exe";
    case "get_version_list":
      return versions;
    case "list_loader":
      return loaders[args.kind as string] ?? [];
    case "validate_name":
      return installed.some((v) => v.name === args.name) ? "不可与现有文件夹重名！" : null;
    case "get_tasks":
      return tasks;
    case "list_installed":
      return installed;
    case "verify_version":
      return { total: 3693, missing: 37, unrecoverable: 0, examples: ["a1b2c3", "d4e5f6"] };
    case "run_dir_of":
      return `${args.dir}\\versions\\${args.name}`;
    default:
      return null;
  }
}

export function previewSetup(s: typeof Store) {
  const pages = ["install", "tasks", "installed", "settings"];
  if (pages.includes(hash)) {
    s.page = hash as typeof s.page;
    if (hash === "install") s.installTarget = versions.versions.find((v) => v.id === "1.20.1")!;
  }
}
