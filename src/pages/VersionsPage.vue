<script setup lang="ts">
// 选择版本：最新版本 + 正式版 / 预览版 / 远古版 / 愚人节版 四张折叠卡片（和 PCL 一致）。
import { computed } from "vue";
import type { Category, VersionEntry } from "../api";
import FoldCard from "../components/FoldCard.vue";
import PixelIcon from "../components/PixelIcon.vue";
import { go, loadVersions, store } from "../store";
import { formatDate, iconForVersion } from "../utils";

const CATS: { key: Category; title: string }[] = [
  { key: "release", title: "正式版" },
  { key: "snapshot", title: "预览版" },
  { key: "old", title: "远古版" },
  { key: "aprilFools", title: "愚人节版" },
];

const grouped = computed(() => {
  const list = store.versionList?.versions ?? [];
  return CATS.map((c) => ({ ...c, items: list.filter((v) => v.category === c.key) }));
});

const latest = computed(() => {
  const l = store.versionList;
  if (!l) return [];
  const out: { v: VersionEntry; lore: string }[] = [];
  const rel = l.versions.find((v) => v.id === l.latestRelease);
  const snap = l.versions.find((v) => v.id === l.latestSnapshot);
  if (rel) out.push({ v: rel, lore: `最新正式版，发布于 ${formatDate(rel.releaseTime)}` });
  if (snap && rel && snap.releaseTime > rel.releaseTime) out.push({ v: snap, lore: `最新预览版，发布于 ${formatDate(snap.releaseTime)}` });
  return out;
});

function pick(v: VersionEntry) {
  store.installTarget = v;
  go("install");
}
</script>

<template>
  <div class="page">
    <div v-if="store.versionLoading && !store.versionList" class="card loading-row"><span class="spin"></span>正在获取版本列表…</div>
    <div v-else-if="store.versionError && !store.versionList" class="card fail">
      <b>获取版本列表失败</b>
      <span class="muted">{{ store.versionError }}</span>
      <span class="muted">检查网络后重试；如果一直失败，可以在“设置”里切换下载源。</span>
      <div><button class="btn" type="button" @click="loadVersions(true)">重试</button></div>
    </div>
    <template v-else-if="store.versionList">
      <FoldCard title="最新版本" :foldable="false">
        <template #head>
          <button class="link refresh" type="button" :disabled="store.versionLoading" @click.stop="loadVersions(true)">
            {{ store.versionLoading ? "刷新中…" : "刷新" }}
          </button>
        </template>
        <button v-for="x in latest" :key="x.v.id" class="li" type="button" @click="pick(x.v)">
          <PixelIcon :kind="iconForVersion(x.v)" />
          <span class="t"><b>{{ x.v.id }}</b><span>{{ x.lore }}</span></span>
        </button>
      </FoldCard>
      <FoldCard v-for="g in grouped" :key="g.key" :title="g.title" :count="g.items.length" :open="false">
        <button v-for="v in g.items" :key="v.id" class="li" type="button" @click="pick(v)">
          <PixelIcon :kind="iconForVersion(v)" />
          <span class="t"><b>{{ v.id }}</b><span>{{ formatDate(v.releaseTime) }}</span></span>
        </button>
      </FoldCard>
    </template>
  </div>
</template>

<style scoped>
.refresh {
  margin-left: auto;
  font-weight: 400;
}
.fail {
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
</style>
