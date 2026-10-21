<script setup lang="ts">
// 游戏目录选择：自动识别的候选 + 选择文件夹 + 手动粘贴路径，选中后显示检查结果。
import { open } from "@tauri-apps/plugin-dialog";
import { onMounted, ref } from "vue";
import { api, errorText, type DirInfo } from "../api";
import PixelIcon from "./PixelIcon.vue";

const props = defineProps<{ modelValue: string | null; known?: string[] }>();
const emit = defineEmits<{ "update:modelValue": [string]; info: [DirInfo] }>();

const candidates = ref<DirInfo[]>([]);
const detecting = ref(true);
const manual = ref("");
const manualError = ref("");
const checking = ref(false);

function add(info: DirInfo) {
  if (!candidates.value.some((c) => c.path.toLowerCase() === info.path.toLowerCase())) candidates.value.push(info);
}

function choose(info: DirInfo) {
  emit("update:modelValue", info.path);
  emit("info", info);
}

onMounted(async () => {
  try {
    const found = await api.detectGameDirs();
    for (const p of props.known ?? []) {
      if (!found.some((f) => f.path.toLowerCase() === p.toLowerCase())) found.unshift(await api.inspectGameDir(p));
    }
    if (props.modelValue && !found.some((f) => f.path.toLowerCase() === props.modelValue!.toLowerCase())) {
      found.unshift(await api.inspectGameDir(props.modelValue));
    }
    candidates.value = found;
    const current = found.find((f) => f.path === props.modelValue);
    if (current) emit("info", current);
    else {
      const best = found.find((f) => f.isPcl && f.writable) ?? found.find((f) => f.writable);
      if (best && !props.modelValue) choose(best);
    }
  } finally {
    detecting.value = false;
  }
});

async function browse() {
  const picked = await open({ directory: true, title: "选择游戏目录（.minecraft 文件夹）" });
  if (typeof picked === "string") await usePath(picked);
}

async function usePath(path: string) {
  manualError.value = "";
  const p = path.trim().replace(/^"|"$/g, "");
  if (!p) {
    manualError.value = "请先输入路径";
    return;
  }
  if (!/^([a-zA-Z]:[\\/]|\/|\\\\)/.test(p)) {
    manualError.value = "请输入完整路径，例如 D:\\Games\\PCL\\.minecraft";
    return;
  }
  checking.value = true;
  try {
    const info = await api.inspectGameDir(p);
    add(info);
    if (info.writable) {
      choose(info);
      manual.value = "";
    } else manualError.value = info.message;
  } catch (e) {
    manualError.value = errorText(e);
  } finally {
    checking.value = false;
  }
}
</script>

<template>
  <div class="picker">
    <div v-if="detecting" class="loading-row"><span class="spin"></span>正在查找 PCL 和已有的游戏目录…</div>
    <template v-else>
      <p v-if="!candidates.length" class="hint warn">
        没有自动找到游戏目录。用 PCL 的话，选 PCL.exe 所在文件夹里的 <b>.minecraft</b>；也可以把本程序放到 PCL.exe 旁边再打开，就能自动识别。
      </p>
      <button
        v-for="c in candidates"
        :key="c.path"
        type="button"
        class="cand"
        :class="{ on: c.path === modelValue, bad: !c.writable }"
        :disabled="!c.writable"
        @click="choose(c)"
      >
        <span class="radio" aria-hidden="true"></span>
        <PixelIcon :kind="c.isPcl ? 'grass' : 'folder'" :size="26" />
        <span class="t">
          <b class="mono">{{ c.path }}</b>
          <span>
            <span class="badge" :class="c.isPcl ? '' : 'gray'">{{ c.isPcl ? "PCL" : c.source }}</span>
            {{ c.message }}
          </span>
        </span>
      </button>
    </template>

    <div class="manual">
      <button class="btn" type="button" @click="browse">选择文件夹…</button>
      <input
        v-model="manual"
        class="input mono"
        :class="{ bad: manualError }"
        placeholder="或粘贴路径，例如 D:\Games\PCL\.minecraft"
        spellcheck="false"
        @keydown.enter="usePath(manual)"
      />
      <button class="btn" type="button" :disabled="checking || !manual.trim()" @click="usePath(manual)">
        <span v-if="checking" class="spin"></span>使用这个路径
      </button>
    </div>
    <p v-if="manualError" class="err">{{ manualError }}</p>
  </div>
</template>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.cand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 12px;
  border-radius: 6px;
  border: 1px solid var(--edge);
  background: var(--card-solid);
  transition: border-color 0.12s, background 0.12s;
}
.cand:hover:not(:disabled) {
  border-color: var(--blue);
}
.cand.on {
  border-color: var(--blue);
  background: var(--blue-soft);
}
.cand.bad {
  opacity: 0.6;
}
.radio {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1.5px solid var(--off);
  flex: none;
  position: relative;
}
.on .radio {
  border-color: var(--blue);
}
.on .radio::after {
  content: "";
  position: absolute;
  inset: 2.5px;
  border-radius: 50%;
  background: var(--blue);
}
.t {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
.t b {
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.t > span {
  font-size: 12px;
  color: var(--sub);
  display: flex;
  align-items: center;
  gap: 6px;
}
.manual {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 4px;
}
.manual .input {
  flex: 1;
  min-width: 0;
}
.err {
  margin: 0;
  color: var(--bad);
  font-size: 12px;
}
</style>
