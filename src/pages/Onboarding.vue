<script setup lang="ts">
// 首次引导：游戏目录 → Java → 下载设置。
import { computed, ref } from "vue";
import type { DirInfo, Source } from "../api";
import GameDirPicker from "../components/GameDirPicker.vue";
import JavaPicker from "../components/JavaPicker.vue";
import { go, loadVersions, refreshJava, saveSettings, store, useGameDir } from "../store";
import { LANGUAGES } from "../utils";

const step = ref(0);
const dir = ref<string | null>(store.settings?.currentDir ?? null);
const dirInfo = ref<DirInfo | null>(null);
const STEPS = ["游戏目录", "Java", "下载设置"];

const canNext = computed(() => (step.value === 0 ? !!dir.value && !!dirInfo.value?.writable : true));

async function next() {
  if (step.value === 0 && dir.value) {
    await useGameDir(dir.value);
    if (!store.java.length && !store.javaLoading) refreshJava(false);
  }
  if (step.value < STEPS.length - 1) {
    step.value++;
    return;
  }
  await saveSettings({ onboarded: true });
  loadVersions(false);
  go("versions");
}

function setSource(s: Source) {
  saveSettings({ source: s });
}
</script>

<template>
  <div class="ob">
    <div class="inner">
      <ol class="steps">
        <li v-for="(s, i) in STEPS" :key="s" :class="{ on: i === step, done: i < step }">
          <span class="n">{{ i < step ? "✓" : i + 1 }}</span>{{ s }}
        </li>
      </ol>

      <section v-if="step === 0" class="card body">
        <h2>游戏装到哪里？</h2>
        <p class="lead">
          选一个 <b>.minecraft</b> 文件夹。如果你用 PCL，选 PCL 的游戏目录，装好的版本会直接出现在 PCL 的版本列表里，打开 PCL 就能启动。
        </p>
        <GameDirPicker v-model="dir" :known="store.settings?.gameDirs" @info="dirInfo = $event" />
      </section>

      <section v-else-if="step === 1" class="card body">
        <h2>Java</h2>
        <p class="lead">
          不同版本的 Minecraft 需要不同的 Java：1.16.5 及更早用 Java 8，1.18 – 1.20.4 用 Java 17，1.20.5 起用 Java 21，26.x 用 Java 25。
          下面是在这台电脑上找到的 Java。安装时会自动挑选合适的版本。
        </p>
        <JavaPicker />
      </section>

      <section v-else class="card body">
        <h2>下载设置</h2>
        <p class="lead">以后可以在“设置”里修改。</p>
        <div class="opts">
          <button type="button" class="opt" :class="{ on: store.settings?.source === 'bmclapi' }" @click="setSource('bmclapi')">
            <span class="radio"></span>
            <span><b>BMCLAPI 镜像优先（推荐）</b><span class="muted">国内速度快很多，镜像出错时自动改用官方源</span></span>
          </button>
          <button type="button" class="opt" :class="{ on: store.settings?.source === 'official' }" @click="setSource('official')">
            <span class="radio"></span>
            <span><b>只用官方源</b><span class="muted">直接从 Mojang 和各加载器官网下载，海外网络更稳定</span></span>
          </button>
        </div>
        <div class="row">
          <span class="k">版本隔离</span>
          <span class="v muted">每个版本的存档、模组和设置放在各自的版本文件夹里，互不影响</span>
          <button
            type="button"
            class="switch"
            :class="{ on: store.settings?.isolate }"
            role="switch"
            :aria-checked="store.settings?.isolate"
            @click="saveSettings({ isolate: !store.settings!.isolate })"
          ></button>
        </div>
        <div class="row">
          <span class="k">游戏语言</span>
          <span class="v muted">写进新版本的 options.txt，进游戏就是这个语言</span>
          <select class="input lang" :value="store.settings?.language" @change="saveSettings({ language: ($event.target as HTMLSelectElement).value })">
            <option v-for="l in LANGUAGES" :key="l.value" :value="l.value">{{ l.label }}</option>
          </select>
        </div>
      </section>

      <div class="foot">
        <button v-if="step > 0" class="btn" type="button" @click="step--">上一步</button>
        <span class="grow"></span>
        <span v-if="step === 0 && dirInfo && !dirInfo.writable" class="err">{{ dirInfo.message }}</span>
        <button class="btn primary" type="button" :disabled="!canNext" @click="next">
          {{ step === STEPS.length - 1 ? "完成，开始使用" : "下一步" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ob {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  display: flex;
  justify-content: center;
  padding: 22px 24px 30px;
}
.inner {
  width: 100%;
  max-width: 720px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  gap: 24px;
  justify-content: center;
}
.steps li {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--sub);
}
.steps li.on {
  color: var(--blue);
  font-weight: 700;
}
.steps li.done {
  color: var(--ok);
}
.n {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  display: inline-grid;
  place-items: center;
  font-size: 12px;
  border: 1.5px solid currentColor;
}
.on .n {
  background: var(--blue);
  border-color: var(--blue);
  color: #fff;
}
.body {
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 17px;
}
.lead {
  margin: 0;
  color: #5d6676;
  line-height: 1.7;
}
.opts {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.opt {
  display: flex;
  gap: 12px;
  align-items: center;
  padding: 10px 12px;
  border: 1px solid var(--edge);
  border-radius: 6px;
  background: var(--card-solid);
}
.opt.on {
  border-color: var(--blue);
  background: var(--blue-soft);
}
.opt > span:last-child {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.opt b {
  font-weight: 500;
}
.opt .muted {
  font-size: 12px;
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
.row {
  display: grid;
  grid-template-columns: 80px 1fr auto;
  align-items: center;
  gap: 12px;
  padding: 10px 2px 0;
  border-top: 1px solid var(--line);
}
.k {
  font-weight: 700;
}
.v {
  font-size: 12.5px;
}
.lang {
  width: 170px;
}
.foot {
  display: flex;
  align-items: center;
  gap: 10px;
}
.grow {
  flex: 1;
}
.err {
  color: var(--bad);
  font-size: 12px;
}
</style>
