<script setup lang="ts">
// 安装配置：版本名称 + 一行一个加载器（行内展开选版本）+ 安装选项 + 开始下载。
import { computed, onMounted, reactive, ref, watch } from "vue";
import { api, errorText, type InstallRequest, type LoaderKind, type LoaderVersion } from "../api";
import PixelIcon from "../components/PixelIcon.vue";
import { go, loaderList, saveSettings, store, toast } from "../store";
import { formatDate, iconForVersion, javaFits, javaFor, LANGUAGES } from "../utils";

const target = computed(() => store.installTarget!);
const mc = computed(() => target.value.id);

interface Row {
  kind: LoaderKind;
  name: string;
}
const ROWS: Row[] = [
  { kind: "forge", name: "Forge" },
  { kind: "neoForge", name: "NeoForge" },
  { kind: "fabric", name: "Fabric" },
  { kind: "fabricApi", name: "Fabric API" },
  { kind: "quilt", name: "Quilt" },
  { kind: "optiFine", name: "OptiFine" },
];
const BASES: LoaderKind[] = ["forge", "neoForge", "fabric", "quilt"];
const NAME: Record<LoaderKind, string> = Object.fromEntries(ROWS.map((r) => [r.kind, r.name])) as Record<LoaderKind, string>;

const sel = reactive<Partial<Record<LoaderKind, LoaderVersion>>>({});
interface ListState {
  loading: boolean;
  error: string;
  items: LoaderVersion[] | null;
}
const lists = reactive<Record<string, ListState>>({});
const openKind = ref<LoaderKind | null>(null);
const showAll = ref(false);

const name = ref("");
const nameEdited = ref(false);
const nameError = ref<string | null>(null);
const isolated = ref(store.settings?.isolate ?? true);
const language = ref(store.settings?.language ?? "zh_cn");
const javaChoice = ref("auto");
const submitting = ref(false);

const gameDir = computed(() => store.settings?.currentDir ?? "");

function load(kind: LoaderKind) {
  const st: ListState = { loading: true, error: "", items: null };
  lists[kind] = st;
  loaderList(kind, mc.value)
    .then((items) => {
      st.items = items;
    })
    .catch((e) => {
      st.error = errorText(e);
    })
    .finally(() => {
      st.loading = false;
      lists[kind] = { ...st };
    });
}

onMounted(() => {
  for (const r of ROWS) load(r.kind);
});

const base = computed(() => BASES.find((b) => sel[b]));

function reason(kind: LoaderKind): string {
  const b = base.value;
  if (BASES.includes(kind) && b && b !== kind) return `与 ${NAME[b]} 不兼容`;
  if (BASES.includes(kind) && kind !== "forge" && sel.optiFine) return "与 OptiFine 不兼容";
  if (kind === "optiFine" && b && b !== "forge") return `与 ${NAME[b]} 不兼容`;
  if (kind === "fabricApi" && !sel.fabric) return "需要先选择 Fabric";
  const st = lists[kind];
  if (!st || st.loading) return "";
  if (st.error) return "";
  if (st.items && st.items.length === 0) return "没有可用版本";
  return "";
}

function statusText(kind: LoaderKind): string {
  const st = lists[kind];
  if (st?.loading) return "正在获取版本列表…";
  if (st?.error) return "获取失败，点击重试";
  return "可以添加";
}

function toggle(kind: LoaderKind) {
  if (reason(kind)) return;
  const st = lists[kind];
  if (st?.error) {
    load(kind);
    return;
  }
  showAll.value = false;
  openKind.value = openKind.value === kind ? null : kind;
}

function choose(kind: LoaderKind, v: LoaderVersion) {
  sel[kind] = v;
  openKind.value = null;
}

function clear(kind: LoaderKind) {
  delete sel[kind];
  if (kind === "fabric") delete sel.fabricApi;
  if (openKind.value === kind) openKind.value = null;
}

const recommended = (kind: LoaderKind) => (lists[kind]?.items ?? []).filter((v) => v.tag);
const visible = (kind: LoaderKind) => {
  const items = lists[kind]?.items ?? [];
  return showAll.value ? items : items.slice(0, 40);
};

function lore(kind: LoaderKind, v: LoaderVersion, withTag: boolean): string {
  const parts: string[] = [];
  if (withTag && v.tag) parts.push(`${v.tag}版`.replace("版版", "版"));
  if (!v.stable && kind !== "optiFine") parts.push("测试版");
  if (!v.stable && kind === "optiFine") parts.push("预览版");
  if (v.note) parts.push(v.note);
  if (v.date) parts.push(`发布于 ${formatDate(v.date)}`);
  return parts.join("，") || (v.stable ? "稳定版" : "");
}

const autoName = computed(() => {
  let n = mc.value;
  if (sel.forge) n += `-Forge_${sel.forge.version}`;
  if (sel.neoForge) n += `-NeoForge_${sel.neoForge.version}`;
  if (sel.fabric) n += `-Fabric ${sel.fabric.version}`;
  if (sel.quilt) n += `-Quilt ${sel.quilt.version}`;
  if (sel.optiFine) n += `-OptiFine_${sel.optiFine.version}`;
  return n;
});
watch(
  autoName,
  (n) => {
    if (!nameEdited.value) name.value = n;
  },
  { immediate: true },
);

let validateTimer: number | undefined;
watch(
  [name, gameDir],
  () => {
    window.clearTimeout(validateTimer);
    validateTimer = window.setTimeout(async () => {
      if (!gameDir.value) {
        nameError.value = null;
        return;
      }
      try {
        nameError.value = await api.validateName(gameDir.value, name.value);
      } catch {
        nameError.value = null;
      }
    }, 200);
  },
  { immediate: true },
);

const chain = computed(() => {
  const parts = [mc.value];
  for (const r of ROWS) if (sel[r.kind]) parts.push(`${r.name} ${sel[r.kind]!.version}`);
  return parts.join("  |  ");
});

const headIcon = computed(() => {
  for (const k of ["forge", "neoForge", "fabric", "quilt", "optiFine"] as LoaderKind[]) if (sel[k]) return k;
  return iconForVersion(target.value);
});

const required = computed(() => javaFor(target.value));
const needsJava = computed(() => !!sel.forge || !!sel.neoForge || (!!sel.optiFine && !sel.forge));
const autoJavaPick = computed(() => {
  const ok = store.java.filter((j) => javaFits(j.major, required.value));
  ok.sort((a, b) => Number(a.major !== required.value) - Number(b.major !== required.value) || a.major - b.major || Number(b.is64bit) - Number(a.is64bit));
  return ok[0] ?? null;
});
const javaLine = computed(() => {
  if (javaChoice.value !== "auto") {
    const j = store.java.find((x) => x.path === javaChoice.value);
    return j ? `使用 Java ${j.version}` : "使用指定的 Java";
  }
  if (autoJavaPick.value) return `需要 Java ${required.value}，将使用本机的 Java ${autoJavaPick.value.version}`;
  if (store.settings?.autoJava) return `需要 Java ${required.value}，本机没有，将自动下载官方 Java`;
  return `需要 Java ${required.value}，本机没有`;
});
const javaBlocked = computed(
  () => javaChoice.value === "auto" && !autoJavaPick.value && !store.settings?.autoJava && needsJava.value,
);

const modTip = computed(() => {
  const b = base.value;
  if (!b) return "";
  return `这个版本装好后只能加载 ${NAME[b]} 模组，其他加载器的模组放进去不会生效。`;
});

const optifineWarn = computed(() => {
  if (!sel.optiFine || !sel.forge) return "";
  const m = /Forge\s+([\d.]+)/.exec(sel.optiFine.note ?? "");
  if (!m) return sel.optiFine.note?.includes("不兼容") ? "这个 OptiFine 版本标注为不兼容 Forge，可能无法启动" : "";
  return `这个 OptiFine 是用 Forge ${m[1]} 测试的，Forge 版本差太多可能无法启动`;
});

const canStart = computed(() => !!gameDir.value && !nameError.value && !!name.value && !submitting.value && !javaBlocked.value);

async function start() {
  if (!canStart.value) return;
  submitting.value = true;
  const api_ = sel.fabricApi;
  const req: InstallRequest = {
    gameDir: gameDir.value,
    mcVersion: mc.value,
    name: name.value,
    forge: sel.forge?.id ?? null,
    neoforge: sel.neoForge?.id ?? null,
    fabric: sel.fabric?.id ?? null,
    fabricApi: api_ && api_.url ? { fileName: api_.id, url: api_.url, sha1: api_.sha1, version: api_.version } : null,
    quilt: sel.quilt?.id ?? null,
    optifine: sel.optiFine?.id ?? null,
    isolated: isolated.value,
    language: language.value || null,
    javaPath: javaChoice.value === "auto" ? null : javaChoice.value,
    autoJava: store.settings?.autoJava ?? true,
  };
  try {
    await api.startInstall(req);
    toast(`已开始安装 ${req.name}`);
    go("tasks");
  } catch (e) {
    toast(errorText(e), "bad");
  } finally {
    submitting.value = false;
  }
}

async function changeDir(e: Event) {
  const v = (e.target as HTMLSelectElement).value;
  if (v === "__manage") {
    go("settings");
    return;
  }
  await saveSettings({ currentDir: v });
}
</script>

<template>
  <div class="page">
    <div class="card head">
      <button class="icon-btn" type="button" aria-label="返回版本列表" @click="go('versions')">←</button>
      <PixelIcon :kind="headIcon" :size="36" />
      <div class="t">
        <input
          v-model="name"
          class="name"
          :class="{ bad: nameError }"
          spellcheck="false"
          aria-label="版本名称"
          @input="nameEdited = true"
        />
        <span v-if="nameError" class="err">{{ nameError }}</span>
        <span v-else class="chain">{{ chain }}</span>
      </div>
      <button v-if="nameEdited" class="link reset" type="button" @click="nameEdited = false; name = autoName">恢复默认名称</button>
    </div>

    <div class="card">
      <div class="irow static">
        <span class="k">Minecraft</span>
        <span class="v"><PixelIcon :kind="iconForVersion(target)" :size="18" />{{ mc }}</span>
        <span class="acts"><button class="link" type="button" @click="go('versions')">修改</button></span>
      </div>
    </div>

    <template v-for="r in ROWS" :key="r.kind">
      <div v-if="r.kind !== 'fabricApi' || sel.fabric" class="card ld" :class="{ open: openKind === r.kind, dis: !!reason(r.kind) }">
        <div
          class="irow"
          role="button"
          :tabindex="reason(r.kind) ? -1 : 0"
          @click="toggle(r.kind)"
          @keydown.enter="toggle(r.kind)"
        >
          <span class="k">{{ r.name }}</span>
          <span v-if="reason(r.kind)" class="v muted">{{ reason(r.kind) }}</span>
          <span v-else-if="sel[r.kind]" class="v"><PixelIcon :kind="r.kind" :size="18" />{{ sel[r.kind]!.version }}</span>
          <span v-else class="v muted">
            <span v-if="lists[r.kind]?.loading" class="spin small"></span>{{ statusText(r.kind) }}
          </span>
          <span v-if="!reason(r.kind)" class="acts">
            <button v-if="sel[r.kind]" class="icon-btn" type="button" :aria-label="`取消 ${r.name}`" @click.stop="clear(r.kind)">✕</button>
            <span class="chev">›</span>
          </span>
        </div>
        <div v-if="openKind === r.kind" class="exp">
          <template v-if="recommended(r.kind).length">
            <button v-for="v in recommended(r.kind)" :key="'r' + v.id" class="li" type="button" @click="choose(r.kind, v)">
              <PixelIcon :kind="r.kind" :size="24" />
              <span class="t"><b>{{ v.version }}</b><span>{{ lore(r.kind, v, true) }}</span></span>
              <span class="r"><span v-if="sel[r.kind]?.id === v.id" class="badge">已选择</span></span>
            </button>
          </template>
          <div class="sub">全部版本 ({{ lists[r.kind]?.items?.length ?? 0 }})</div>
          <button v-for="v in visible(r.kind)" :key="v.id" class="li" type="button" @click="choose(r.kind, v)">
            <PixelIcon :kind="r.kind" :size="24" />
            <span class="t"><b>{{ v.version }}</b><span>{{ lore(r.kind, v, false) }}</span></span>
            <span class="r">
              <span v-if="v.tag" class="badge" :class="{ g: v.tag.includes('最新') }">{{ v.tag }}</span>
              <span v-if="sel[r.kind]?.id === v.id" class="badge">已选择</span>
            </span>
          </button>
          <button
            v-if="!showAll && (lists[r.kind]?.items?.length ?? 0) > 40"
            class="link more"
            type="button"
            @click="showAll = true"
          >
            显示全部 {{ lists[r.kind]?.items?.length }} 个版本
          </button>
        </div>
      </div>
    </template>

    <p v-if="modTip" class="hint warn">提示：{{ modTip }}</p>
    <p v-if="optifineWarn" class="hint warn">{{ optifineWarn }}</p>

    <div class="card opts">
      <div class="card-h">安装选项</div>
      <div class="orow">
        <span class="k">游戏目录</span>
        <select class="input mono" :value="gameDir" @change="changeDir">
          <option v-for="d in store.settings?.gameDirs ?? []" :key="d" :value="d">{{ d }}</option>
          <option value="__manage">添加或管理目录…</option>
        </select>
      </div>
      <div class="orow">
        <span class="k">版本隔离</span>
        <span class="v muted">存档、模组和设置放在这个版本自己的文件夹里</span>
        <button type="button" class="switch" :class="{ on: isolated }" role="switch" :aria-checked="isolated" @click="isolated = !isolated"></button>
      </div>
      <div class="orow">
        <span class="k">游戏语言</span>
        <select v-model="language" class="input small">
          <option v-for="l in LANGUAGES" :key="l.value" :value="l.value">{{ l.label }}</option>
        </select>
      </div>
      <div class="orow">
        <span class="k">Java</span>
        <span class="v" :class="{ warnt: javaBlocked }">{{ javaLine }}</span>
        <select v-model="javaChoice" class="input small">
          <option value="auto">自动选择</option>
          <option v-for="j in store.java" :key="j.path" :value="j.path">
            Java {{ j.version }}{{ javaFits(j.major, required) ? "" : "（版本不匹配）" }}
          </option>
        </select>
      </div>
      <p v-if="javaBlocked" class="hint bad jb">
        安装 {{ sel.forge ? "Forge" : sel.neoForge ? "NeoForge" : "OptiFine" }} 需要 Java {{ required }}。请到“设置 → Java”添加 java.exe，或开启“缺少时自动下载”。
      </p>
    </div>

    <button class="fab" type="button" :disabled="!canStart" @click="start">
      <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M7 1v8M3.5 5.5 7 9l3.5-3.5M2 12.5h10" fill="none" stroke="currentColor" stroke-width="1.6" /></svg>
      开始下载
    </button>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 18px 12px 10px;
  border-color: rgba(19, 112, 243, 0.45);
  box-shadow: 0 3px 12px -4px rgba(19, 112, 243, 0.35);
}
.head .t {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  min-width: 0;
}
.name {
  width: 100%;
  border: 1px solid var(--edge);
  border-bottom: 2px solid var(--blue);
  border-radius: 4px 4px 2px 2px;
  padding: 4px 9px;
  font: inherit;
  font-size: 15px;
  font-weight: 500;
  color: var(--ink);
  background: var(--card-solid);
  outline: none;
}
.name.bad {
  border-color: var(--bad);
}
.chain,
.err {
  padding-left: 10px;
  font-size: 12.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.chain {
  color: var(--sub);
}
.err {
  color: var(--bad);
}
.reset {
  flex: none;
}
.irow {
  display: grid;
  grid-template-columns: 150px 1fr auto;
  align-items: center;
  min-height: 50px;
  padding: 0 10px 0 18px;
  width: 100%;
  cursor: pointer;
  border-radius: 6px;
}
.irow.static {
  cursor: default;
}
.ld:not(.dis) .irow:hover {
  background: var(--hover);
}
.ld.dis .irow {
  cursor: default;
}
.ld.dis .k {
  color: var(--faint);
}
.ld.open {
  border-color: rgba(19, 112, 243, 0.4);
}
.k {
  font-weight: 700;
  font-size: 13.5px;
}
.v {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13.5px;
  min-width: 0;
}
.v.muted {
  color: var(--faint);
}
.acts {
  display: flex;
  align-items: center;
  gap: 2px;
}
.chev {
  width: 30px;
  text-align: center;
  color: #5d6676;
  font-size: 17px;
  transition: transform 0.2s;
}
.open .chev {
  transform: rotate(90deg);
}
.spin.small {
  width: 11px;
  height: 11px;
}
.exp {
  border-top: 1px solid var(--line);
  padding: 6px 6px 10px;
  display: flex;
  flex-direction: column;
}
.sub {
  font-size: 12px;
  color: var(--sub);
  padding: 10px 10px 4px;
}
.more {
  align-self: center;
  margin-top: 6px;
}
.opts {
  padding-bottom: 6px;
}
.opts .card-h {
  padding-bottom: 4px;
}
.orow {
  display: grid;
  grid-template-columns: 150px 1fr auto;
  align-items: center;
  gap: 0 12px;
  min-height: 44px;
  padding: 0 16px 0 18px;
  border-top: 1px solid var(--line);
}
.opts .card-h + .orow {
  border-top: 0;
}
.orow .k {
  font-size: 13px;
}
.orow > select.input:not(.small) {
  grid-column: 2 / 4;
}
.orow .v {
  font-size: 12.5px;
  color: #5d6676;
}
.orow .v.warnt {
  color: var(--bad);
}
.input.small {
  width: 190px;
}
.jb {
  margin: 4px 16px 6px;
}
.fab {
  position: fixed;
  left: calc(50% + 70px);
  bottom: 20px;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 32px;
  border-radius: 999px;
  background: var(--blue);
  color: #fff;
  font-weight: 700;
  font-size: 14px;
  box-shadow: 0 8px 22px -6px rgba(19, 112, 243, 0.65);
  transition: box-shadow 0.15s, transform 0.15s, background 0.15s;
  z-index: 5;
}
.fab:hover:not(:disabled) {
  transform: translateX(-50%) translateY(-1px);
  box-shadow: 0 12px 26px -6px rgba(19, 112, 243, 0.75);
}
.fab:disabled {
  background: #9db8e4;
  box-shadow: none;
}
p.hint {
  margin: 0;
}
</style>
