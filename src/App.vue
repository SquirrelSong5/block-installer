<script setup lang="ts">
import { computed, onMounted } from "vue";
import TitleBar from "./components/TitleBar.vue";
import InstalledPage from "./pages/InstalledPage.vue";
import InstallPage from "./pages/InstallPage.vue";
import Onboarding from "./pages/Onboarding.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import TasksPage from "./pages/TasksPage.vue";
import VersionsPage from "./pages/VersionsPage.vue";
import { activeTaskCount, go, init, saveSettings, store, toast } from "./store";
import { errorText } from "./api";

onMounted(() => {
  init().catch((e) => toast(`初始化失败：${errorText(e)}`, "bad"));
  // 禁用网页式的右键菜单和刷新快捷键，更像桌面程序
  window.addEventListener("contextmenu", (e) => {
    const el = e.target as HTMLElement;
    if (!el.closest("input, textarea, pre")) e.preventDefault();
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "F5" || (e.ctrlKey && e.key.toLowerCase() === "r")) e.preventDefault();
  });
});

const section = computed(() => {
  if (store.page === "installed") return "installed";
  if (store.page === "settings") return "settings";
  return "download";
});
const running = computed(() => activeTaskCount());

async function switchDir(d: string) {
  await saveSettings({ currentDir: d });
}

function shortName(p: string) {
  const parts = p.split(/[\\/]/).filter(Boolean);
  const last = parts[parts.length - 1] ?? p;
  return last === ".minecraft" && parts.length > 1 ? `${parts[parts.length - 2]}\\.minecraft` : last;
}
</script>

<template>
  <div class="app">
    <TitleBar />
    <div v-if="!store.ready" class="boot"><span class="spin"></span></div>
    <div v-else-if="!store.settings?.onboarded" class="body single">
      <div class="stage"><Onboarding /></div>
    </div>
    <div v-else class="body" :class="{ single: section === 'settings' }">
      <aside v-if="section === 'download'" class="side">
        <div class="grp">Minecraft</div>
        <button type="button" class="it" :class="{ on: store.page === 'versions' || store.page === 'install' }" @click="go('versions')">游戏下载</button>
        <div class="grp gap">任务</div>
        <button type="button" class="it" :class="{ on: store.page === 'tasks' }" @click="go('tasks')">
          任务管理<span v-if="running" class="num">{{ running }}</span>
        </button>
      </aside>
      <aside v-else-if="section === 'installed'" class="side">
        <div class="grp">游戏目录</div>
        <button
          v-for="d in store.settings?.gameDirs ?? []"
          :key="d"
          type="button"
          class="it"
          :class="{ on: d === store.settings?.currentDir }"
          :title="d"
          @click="switchDir(d)"
        >
          <span class="ell">{{ shortName(d) }}</span>
        </button>
        <button type="button" class="it add" @click="go('settings')">＋ 添加目录</button>
      </aside>
      <main class="stage">
        <VersionsPage v-if="store.page === 'versions'" />
        <InstallPage v-else-if="store.page === 'install' && store.installTarget" :key="store.installTarget.id" />
        <TasksPage v-else-if="store.page === 'tasks'" />
        <InstalledPage v-else-if="store.page === 'installed'" />
        <SettingsPage v-else-if="store.page === 'settings'" />
        <VersionsPage v-else />
      </main>
    </div>
    <transition name="toast">
      <div v-if="store.toast.text" :key="store.toast.seq" class="toast" :class="store.toast.kind" role="status">
        {{ store.toast.text }}
      </div>
    </transition>
  </div>
</template>

<style scoped>
.app {
  height: 100%;
  display: flex;
  flex-direction: column;
  border: 1px solid rgba(11, 91, 208, 0.5);
  overflow: hidden;
}
.boot {
  flex: 1;
  display: grid;
  place-items: center;
  background: linear-gradient(135deg, var(--grad-a), var(--grad-b) 48%, var(--grad-c));
}
.body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 140px 1fr;
  background: linear-gradient(135deg, var(--grad-a) 0%, var(--grad-b) 48%, var(--grad-c) 100%);
}
.body.single {
  grid-template-columns: 1fr;
}
.side {
  background: rgba(255, 255, 255, 0.55);
  border-right: 1px solid var(--edge);
  padding: 14px 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.grp {
  font-size: 11px;
  color: var(--sub);
  padding: 4px 18px 6px;
}
.grp.gap {
  margin-top: 12px;
}
.it {
  position: relative;
  padding: 8px 18px;
  color: #4d5768;
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.it:hover {
  color: var(--blue);
}
.it.on {
  color: var(--blue);
  font-weight: 700;
}
.it.on::before {
  content: "";
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 3px;
  border-radius: 0 3px 3px 0;
  background: var(--blue);
}
.it.add {
  color: var(--blue);
}
.ell {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.num {
  margin-left: auto;
  min-width: 18px;
  height: 18px;
  border-radius: 9px;
  background: var(--blue);
  color: #fff;
  font-size: 11px;
  display: inline-grid;
  place-items: center;
  padding: 0 5px;
  font-weight: 400;
}
.stage {
  position: relative;
  min-width: 0;
  min-height: 0;
}
.toast {
  position: fixed;
  left: 50%;
  bottom: 74px;
  transform: translateX(-50%);
  padding: 8px 18px;
  border-radius: 999px;
  background: rgba(30, 40, 60, 0.88);
  color: #fff;
  font-size: 12.5px;
  z-index: 100;
  max-width: 80%;
  box-shadow: 0 6px 20px -8px rgba(0, 0, 0, 0.4);
}
.toast.ok {
  background: rgba(24, 130, 70, 0.92);
}
.toast.bad {
  background: rgba(190, 50, 50, 0.94);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.2s, transform 0.2s;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(6px);
}
</style>
