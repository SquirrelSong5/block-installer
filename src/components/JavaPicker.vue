<script setup lang="ts">
// Java 管理：显示扫描结果、重新扫描、手动添加 java.exe、自动下载开关。
import { open } from "@tauri-apps/plugin-dialog";
import { ref } from "vue";
import { api, errorText } from "../api";
import PixelIcon from "./PixelIcon.vue";
import { refreshJava, saveSettings, store, toast } from "../store";
import { javaCoverage } from "../utils";

const manual = ref("");
const error = ref("");
const checking = ref(false);

async function browse() {
  const picked = await open({
    title: "选择 java.exe（在 Java 安装目录的 bin 文件夹里）",
    filters: [{ name: "Java", extensions: ["exe"] }],
  });
  if (typeof picked === "string") await addPath(picked);
}

async function addPath(path: string) {
  error.value = "";
  if (!path.trim()) return;
  checking.value = true;
  try {
    const info = await api.probeJava(path);
    const paths = store.settings!.javaPaths.filter((p) => p.toLowerCase() !== info.path.toLowerCase());
    await saveSettings({ javaPaths: [...paths, info.path] });
    await refreshJava(true);
    manual.value = "";
    toast(`已添加 Java ${info.version}`, "ok");
  } catch (e) {
    error.value = errorText(e);
  } finally {
    checking.value = false;
  }
}

async function remove(path: string) {
  await saveSettings({ javaPaths: store.settings!.javaPaths.filter((p) => p !== path) });
  await refreshJava(true);
}

function isManual(path: string) {
  return store.settings?.javaPaths.some((p) => p.toLowerCase() === path.toLowerCase()) ?? false;
}
</script>

<template>
  <div class="jp">
    <div class="head">
      <span class="muted">已找到 {{ store.java.length }} 个 Java</span>
      <button class="link" type="button" :disabled="store.javaLoading" @click="refreshJava(true)">
        {{ store.javaLoading ? "正在扫描…" : "重新扫描" }}
      </button>
    </div>
    <div v-if="store.javaLoading && !store.java.length" class="loading-row"><span class="spin"></span>正在扫描本机的 Java…</div>
    <p v-else-if="!store.java.length" class="hint info">
      没有找到 Java。不用担心：开启下面的自动下载后，安装时会下载对应版本的官方 Java（和 PCL、官方启动器用的是同一套），放在
      <span class="mono">%APPDATA%\.minecraft\runtime</span>，PCL 也能直接用。
    </p>
    <div v-for="j in store.java" :key="j.path" class="li">
      <PixelIcon kind="java" :size="26" />
      <span class="t">
        <b>Java {{ j.major }} <span class="ver">{{ j.version }}{{ j.is64bit ? "" : "（32 位）" }}</span></b>
        <span class="mono">{{ j.path }}</span>
      </span>
      <span class="r">
        <span class="badge gray">{{ j.source }}</span>
        <span class="cov">{{ javaCoverage(j.major) }}</span>
        <button v-if="isManual(j.path)" class="link" type="button" @click="remove(j.path)">移除</button>
      </span>
    </div>

    <div class="manual">
      <button class="btn" type="button" @click="browse">添加 java.exe…</button>
      <input
        v-model="manual"
        class="input mono"
        :class="{ bad: error }"
        placeholder="或粘贴路径，例如 C:\Program Files\Java\jdk-21\bin\java.exe"
        spellcheck="false"
        @keydown.enter="addPath(manual)"
      />
      <button class="btn" type="button" :disabled="checking || !manual.trim()" @click="addPath(manual)">
        <span v-if="checking" class="spin"></span>添加
      </button>
    </div>
    <p v-if="error" class="err">{{ error }}</p>

    <label class="auto">
      <button
        type="button"
        class="switch"
        :class="{ on: store.settings?.autoJava }"
        role="switch"
        :aria-checked="store.settings?.autoJava"
        @click="saveSettings({ autoJava: !store.settings!.autoJava })"
      ></button>
      <span>
        <b>缺少时自动下载官方 Java</b>
        <span class="muted">推荐开启。Forge、NeoForge 和单独安装 OptiFine 时必须有 Java。</span>
      </span>
    </label>
  </div>
</template>

<style scoped>
.jp {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 4px;
  font-size: 12.5px;
}
.ver {
  color: var(--sub);
  font-weight: 400;
  font-size: 12px;
  margin-left: 4px;
}
.cov {
  font-size: 12px;
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
.auto {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  margin-top: 8px;
  padding: 10px 12px;
  border-radius: 6px;
  background: var(--card-solid);
  border: 1px solid var(--edge);
}
.auto .switch {
  margin-top: 2px;
}
.auto > span {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.auto b {
  font-weight: 500;
}
.auto .muted {
  font-size: 12px;
}
</style>
