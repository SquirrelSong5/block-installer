<script setup lang="ts">
// 已安装版本：检查文件、补全、打开文件夹、删除。
import { computed, onMounted, ref, watch } from "vue";
import { api, errorText, type DirInfo, type InstalledVersion, type VerifyReport } from "../api";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import PixelIcon from "../components/PixelIcon.vue";
import { go, store, toast } from "../store";

const dir = computed(() => store.settings?.currentDir ?? "");
const list = ref<InstalledVersion[]>([]);
const loading = ref(false);
const info = ref<DirInfo | null>(null);
const reports = ref<Record<string, VerifyReport | "checking" | string>>({});
const deleting = ref<InstalledVersion | null>(null);

async function refresh() {
  if (!dir.value) return;
  loading.value = true;
  try {
    const [l, i] = await Promise.all([api.listInstalled(dir.value), api.inspectGameDir(dir.value)]);
    list.value = l;
    info.value = i;
  } finally {
    loading.value = false;
  }
}
onMounted(refresh);
watch(dir, () => {
  reports.value = {};
  refresh();
});
// 有任务完成后刷新列表
watch(
  () => store.tasks.filter((t) => t.status === "done").length,
  () => refresh(),
);

function icon(v: InstalledVersion) {
  const l = v.loaders.join(" ");
  if (l.includes("NeoForge")) return "neoForge";
  if (l.includes("Forge")) return "forge";
  if (l.includes("Fabric")) return "fabric";
  if (l.includes("Quilt")) return "quilt";
  if (l.includes("OptiFine")) return "optiFine";
  return "grass";
}

function sub(v: InstalledVersion) {
  if (v.problem) return v.problem;
  return [v.mcVersion || "未知版本", ...(v.loaders.length ? v.loaders : ["原版"]), v.isolated ? "已隔离" : "未隔离"].join("  |  ");
}

async function check(v: InstalledVersion) {
  reports.value[v.name] = "checking";
  try {
    reports.value[v.name] = await api.verifyVersion(dir.value, v.name);
  } catch (e) {
    reports.value[v.name] = errorText(e);
  }
}

async function repair(v: InstalledVersion) {
  try {
    await api.startRepair(dir.value, v.name);
    delete reports.value[v.name];
    toast(`开始补全 ${v.name}`);
    go("tasks");
  } catch (e) {
    toast(errorText(e), "bad");
  }
}

async function openDir(v: InstalledVersion) {
  try {
    await api.openFolder(await api.runDirOf(dir.value, v.name, true));
  } catch (e) {
    toast(errorText(e), "bad");
  }
}

async function confirmDelete() {
  const v = deleting.value;
  deleting.value = null;
  if (!v) return;
  try {
    await api.deleteVersion(dir.value, v.name);
    toast(`已删除 ${v.name}`, "ok");
    refresh();
  } catch (e) {
    toast(errorText(e), "bad");
  }
}

function report(v: InstalledVersion) {
  return reports.value[v.name];
}
</script>

<template>
  <div class="page">
    <p v-if="info?.isPcl" class="hint info"><b>这是 PCL 的游戏目录</b>　这里安装的版本会直接出现在 PCL 的版本列表里。</p>
    <p v-else-if="info && !info.exists" class="hint warn">目录还不存在，安装第一个版本后会自动创建。</p>

    <div class="card">
      <div class="card-h">
        <span class="mono path">{{ dir }}</span>
        <span class="cnt">({{ list.length }})</span>
        <span class="grow"></span>
        <button class="link" type="button" @click="api.openFolder(dir)">打开目录</button>
        <button class="link" type="button" :disabled="loading" @click="refresh">刷新</button>
      </div>
      <div class="card-b">
        <div v-if="loading && !list.length" class="loading-row"><span class="spin"></span>正在读取…</div>
        <div v-else-if="!list.length" class="empty muted">
          这个目录里还没有版本。<button class="link" type="button" @click="go('versions')">去安装一个</button>
        </div>
        <div v-for="v in list" :key="v.name" class="item">
          <div class="li">
            <PixelIcon :kind="icon(v)" />
            <span class="t">
              <b>{{ v.name }}</b>
              <span :class="{ warnt: v.problem }">{{ sub(v) }}</span>
            </span>
            <span class="r acts">
              <button class="act" type="button" :disabled="report(v) === 'checking' || !!v.problem" @click="check(v)">检查文件</button>
              <button class="act" type="button" @click="openDir(v)">打开文件夹</button>
              <button class="act danger" type="button" @click="deleting = v">删除</button>
            </span>
          </div>
          <div v-if="report(v)" class="result">
            <template v-if="report(v) === 'checking'"><span class="spin"></span>正在检查…</template>
            <template v-else-if="typeof report(v) === 'string'"><span class="warnt">检查失败：{{ report(v) }}</span></template>
            <template v-else-if="(report(v) as VerifyReport).missing === 0">
              <span class="okt">文件完整</span>，共 {{ (report(v) as VerifyReport).total }} 个文件
            </template>
            <template v-else>
              <span class="warnt">缺少 {{ (report(v) as VerifyReport).missing }} 个文件</span>
              <span class="muted">（如 {{ (report(v) as VerifyReport).examples.join("、") }}）</span>
              <button class="btn primary sm" type="button" @click="repair(v)">补全文件</button>
            </template>
          </div>
        </div>
      </div>
    </div>

    <ConfirmDialog
      v-if="deleting"
      title="删除版本"
      :message="`确定删除 ${deleting.name}？\n会删除整个版本文件夹${deleting.isolated ? '，包括里面的存档、模组和设置' : ''}，无法恢复。`"
      confirm-text="删除"
      danger
      @confirm="confirmDelete"
      @cancel="deleting = null"
    />
  </div>
</template>

<style scoped>
.path {
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.grow {
  flex: 1;
}
.card-h .link {
  font-weight: 400;
  margin-left: 10px;
  flex: none;
}
.empty {
  padding: 14px 10px;
}
.acts {
  gap: 2px;
}
.act {
  font-size: 12px;
  color: var(--sub);
  padding: 4px 8px;
  border-radius: 4px;
}
.li:hover .act {
  color: var(--blue);
}
.act:hover {
  background: var(--hover);
}
.li:hover .act.danger:hover {
  color: var(--bad);
}
.result {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px 8px 50px;
  font-size: 12.5px;
}
.okt {
  color: var(--ok);
}
.warnt {
  color: #c0631b !important;
}
.btn.sm {
  padding: 3px 10px;
  font-size: 12px;
}
</style>
