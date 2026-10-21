<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import { computed } from "vue";
import PixelIcon from "./PixelIcon.vue";
import { activeTaskCount, go, store } from "../store";
import { inTauri } from "../api";

const noop = { minimize() {}, toggleMaximize() {}, close() {} };
const win = inTauri ? getCurrentWindow() : noop;
const section = computed(() => {
  if (store.page === "installed") return "installed";
  if (store.page === "settings") return "settings";
  return "download";
});
const running = computed(() => activeTaskCount());
</script>

<template>
  <header class="bar" data-tauri-drag-region>
    <div class="logo" data-tauri-drag-region>
      <PixelIcon kind="grass" :size="20" />
      <span data-tauri-drag-region>方块安装器</span>
    </div>
    <nav v-if="store.settings?.onboarded" class="nav" data-tauri-drag-region>
      <button type="button" :class="{ on: section === 'download' }" @click="go('versions')">
        下载<span v-if="running" class="dot">{{ running }}</span>
      </button>
      <button type="button" :class="{ on: section === 'installed' }" @click="go('installed')">已安装</button>
      <button type="button" :class="{ on: section === 'settings' }" @click="go('settings')">设置</button>
    </nav>
    <div v-else class="nav" data-tauri-drag-region></div>
    <div class="ctl">
      <button type="button" aria-label="最小化" @click="win.minimize()">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button type="button" aria-label="最大化" @click="win.toggleMaximize()">
        <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.6" y="0.6" width="8.8" height="8.8" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button type="button" class="close" aria-label="关闭" @click="win.close()">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.bar {
  height: 48px;
  flex: none;
  display: grid;
  grid-template-columns: 220px 1fr 220px;
  align-items: center;
  padding-left: 16px;
  background: linear-gradient(90deg, var(--blue-d), var(--blue));
  color: #fff;
}
.logo {
  display: flex;
  align-items: center;
  gap: 9px;
  font-weight: 700;
  font-size: 15px;
}
.nav {
  display: flex;
  justify-content: center;
  gap: 4px;
  height: 100%;
  align-items: center;
}
.nav button {
  padding: 4px 18px;
  border-radius: 999px;
  color: rgba(255, 255, 255, 0.9);
  font-weight: 500;
  font-size: 13.5px;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: background 0.15s;
}
.nav button:hover {
  background: rgba(255, 255, 255, 0.15);
}
.nav button.on {
  background: #fff;
  color: var(--blue);
}
.dot {
  min-width: 16px;
  height: 16px;
  border-radius: 8px;
  background: #ff7a45;
  color: #fff;
  font-size: 10.5px;
  display: inline-grid;
  place-items: center;
  padding: 0 4px;
}
.ctl {
  display: flex;
  justify-content: flex-end;
  height: 100%;
}
.ctl button {
  width: 46px;
  height: 100%;
  display: grid;
  place-items: center;
  color: rgba(255, 255, 255, 0.92);
}
.ctl button:hover {
  background: rgba(255, 255, 255, 0.16);
}
.ctl .close:hover {
  background: #e81123;
}
</style>
