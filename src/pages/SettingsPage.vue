<script setup lang="ts">
import { ref } from "vue";
import type { Source } from "../api";
import GameDirPicker from "../components/GameDirPicker.vue";
import JavaPicker from "../components/JavaPicker.vue";
import { saveSettings, store, toast, useGameDir } from "../store";
import { LANGUAGES } from "../utils";

const dir = ref<string | null>(store.settings?.currentDir ?? null);

async function pickDir(path: string) {
  dir.value = path;
  await useGameDir(path);
  toast("已切换游戏目录", "ok");
}

function setSource(s: Source) {
  saveSettings({ source: s });
}

const THREADS = [16, 32, 48, 64, 96];
</script>

<template>
  <div class="page">
    <div class="card sec">
      <div class="card-h">游戏目录</div>
      <p class="desc">新版本会装到选中的目录。用 PCL 的话选 PCL 的 .minecraft。</p>
      <GameDirPicker :model-value="dir" :known="store.settings?.gameDirs" @update:model-value="pickDir" />
    </div>

    <div class="card sec">
      <div class="card-h">Java</div>
      <p class="desc">安装 Forge、NeoForge 时需要用 Java 运行官方安装流程。找不到的 Java 可以手动添加。</p>
      <JavaPicker />
    </div>

    <div class="card sec">
      <div class="card-h">下载</div>
      <div class="row">
        <span class="k">下载源</span>
        <div class="seg">
          <button type="button" :class="{ on: store.settings?.source === 'bmclapi' }" @click="setSource('bmclapi')">BMCLAPI 镜像优先</button>
          <button type="button" :class="{ on: store.settings?.source === 'official' }" @click="setSource('official')">只用官方源</button>
        </div>
      </div>
      <div class="row">
        <span class="k">同时下载</span>
        <select
          class="input small"
          :value="store.settings?.concurrency"
          @change="saveSettings({ concurrency: Number(($event.target as HTMLSelectElement).value) })"
        >
          <option v-for="n in THREADS" :key="n" :value="n">{{ n }} 个文件</option>
        </select>
        <span class="muted note">网络不稳定时调小一些</span>
      </div>
    </div>

    <div class="card sec">
      <div class="card-h">新版本默认设置</div>
      <div class="row">
        <span class="k">版本隔离</span>
        <button
          type="button"
          class="switch"
          :class="{ on: store.settings?.isolate }"
          role="switch"
          :aria-checked="store.settings?.isolate"
          @click="saveSettings({ isolate: !store.settings!.isolate })"
        ></button>
        <span class="muted note">每个版本的存档、模组、设置互不影响（PCL 会读取这个设置）</span>
      </div>
      <div class="row">
        <span class="k">游戏语言</span>
        <select class="input small" :value="store.settings?.language" @change="saveSettings({ language: ($event.target as HTMLSelectElement).value })">
          <option v-for="l in LANGUAGES" :key="l.value" :value="l.value">{{ l.label }}</option>
        </select>
      </div>
    </div>

    <div class="card sec">
      <div class="card-h">关于</div>
      <p class="desc">
        方块安装器 0.1.0。只负责下载和安装 Minecraft 本体与加载器，启动请使用 PCL、HMCL 或官方启动器。<br />
        数据来源：Mojang 官方、BMCLAPI 镜像（bangbang93）、Forge、NeoForge、Fabric、Quilt、Modrinth。
      </p>
      <div class="row">
        <button class="btn" type="button" @click="saveSettings({ onboarded: false })">重新运行首次引导</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sec {
  padding: 0 16px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.sec .card-h {
  padding-inline: 0;
  padding-bottom: 0;
}
.desc {
  margin: 0;
  color: #5d6676;
  font-size: 12.5px;
  line-height: 1.6;
}
.row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 38px;
}
.k {
  width: 80px;
  font-weight: 700;
  flex: none;
}
.note {
  font-size: 12px;
}
.input.small {
  width: 160px;
}
.seg {
  display: inline-flex;
  border: 1px solid var(--edge);
  border-radius: 6px;
  overflow: hidden;
  background: var(--card-solid);
}
.seg button {
  padding: 6px 14px;
  font-size: 12.5px;
  color: #4d5768;
}
.seg button + button {
  border-left: 1px solid var(--edge);
}
.seg button.on {
  background: var(--blue);
  color: #fff;
}
</style>
