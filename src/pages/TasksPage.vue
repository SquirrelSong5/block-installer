<script setup lang="ts">
// 任务管理：左侧总览，右侧每个任务一张卡片。
import { computed, ref } from "vue";
import { api, errorText, type TaskSnapshot } from "../api";
import { go, store, toast } from "../store";
import { formatSpeed } from "../utils";

const current = computed(() => store.tasks.find((t) => t.status === "running") ?? null);
const ordered = computed(() => [...store.tasks].reverse());
const openLogs = ref<Record<number, boolean>>({});

function pct(n: number) {
  return `${(n * 100).toFixed(n >= 1 ? 0 : 1)}%`;
}

function stepLabel(t: TaskSnapshot, s: TaskSnapshot["steps"][number]) {
  if (s.state === "done") return "100%";
  if (s.state === "failed") return "失败";
  if (s.state === "running") return `${Math.floor(s.progress * 100)}%`;
  return t.status === "cancelled" || t.status === "failed" ? "—" : "等待";
}

async function cancel(t: TaskSnapshot) {
  await api.cancelTask(t.id);
}
async function dismiss(t: TaskSnapshot) {
  await api.dismissTask(t.id);
  store.tasks = store.tasks.filter((x) => x.id !== t.id);
}
async function openVersion(t: TaskSnapshot) {
  const dir = store.settings?.currentDir;
  if (!dir) return;
  try {
    await api.openFolder(`${dir}${dir.includes("\\") ? "\\" : "/"}versions${dir.includes("\\") ? "\\" : "/"}${t.versionName}`);
  } catch (e) {
    toast(errorText(e), "bad");
  }
}
async function copyLogs(t: TaskSnapshot) {
  try {
    await navigator.clipboard.writeText([t.error ?? "", ...t.logs].join("\n"));
    toast("日志已复制", "ok");
  } catch {
    toast("复制失败", "bad");
  }
}
</script>

<template>
  <div class="page">
    <div v-if="!store.tasks.length" class="card empty">
      <b>还没有任务</b>
      <span class="muted">在“游戏下载”里选一个版本开始安装，进度会显示在这里。</span>
      <div><button class="btn" type="button" @click="go('versions')">去选版本</button></div>
    </div>
    <div v-else class="wrap">
      <div class="stats">
        <div class="stat"><span class="l">总进度</span><span class="n">{{ current ? pct(current.progress) : "—" }}</span></div>
        <div class="stat"><span class="l">下载速度</span><span class="n">{{ current ? formatSpeed(current.speed) : "—" }}</span></div>
        <div class="stat"><span class="l">剩余文件</span><span class="n">{{ current ? current.remainingFiles.toLocaleString() : "—" }}</span></div>
      </div>
      <div class="list">
        <div v-for="t in ordered" :key="t.id" class="card task" :class="t.status">
          <div class="tt">
            <span class="title">{{ t.title }}</span>
            <span v-if="t.status === 'queued'" class="badge gray">排队中</span>
            <span v-else-if="t.status === 'done'" class="badge g">完成</span>
            <span v-else-if="t.status === 'failed'" class="badge w">失败</span>
            <span v-else-if="t.status === 'cancelled'" class="badge gray">已取消</span>
            <span class="grow"></span>
            <button
              v-if="t.status === 'running' || t.status === 'queued'"
              class="icon-btn"
              type="button"
              aria-label="取消任务"
              title="取消"
              @click="cancel(t)"
            >✕</button>
            <button v-else class="icon-btn" type="button" aria-label="移除任务" title="移除" @click="dismiss(t)">✕</button>
          </div>
          <div v-for="s in t.steps" :key="s.key" class="row" :class="s.state">
            <span class="pc">{{ stepLabel(t, s) }}</span>
            <span class="name">
              {{ s.title }}
              <span v-if="s.state === 'running'" class="bar"><i :style="{ width: `${Math.max(2, s.progress * 100)}%` }"></i></span>
            </span>
            <span class="detail">{{ s.detail }}</span>
          </div>
          <p v-if="t.status === 'done' && t.kind === 'install'" class="hint info done">
            安装完成。打开启动器，在版本列表里选择 <b>{{ t.versionName }}</b> 就能启动。
          </p>
          <p v-if="t.error && t.status === 'failed'" class="hint bad">{{ t.error }}</p>
          <div class="foot">
            <button v-if="t.status === 'done'" class="btn" type="button" @click="openVersion(t)">打开版本文件夹</button>
            <button v-if="t.logs.length" class="link" type="button" @click="openLogs[t.id] = !openLogs[t.id]">
              {{ openLogs[t.id] ? "收起日志" : "查看日志" }}
            </button>
            <button v-if="openLogs[t.id]" class="link" type="button" @click="copyLogs(t)">复制日志</button>
          </div>
          <pre v-if="openLogs[t.id]" class="logs">{{ t.logs.join("\n") }}</pre>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.empty {
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.wrap {
  display: grid;
  grid-template-columns: 170px 1fr;
  gap: 18px;
  align-items: start;
}
.stats {
  display: flex;
  flex-direction: column;
  gap: 26px;
  padding-top: 26px;
  position: sticky;
  top: 0;
}
.stat {
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.stat .l {
  color: var(--blue);
  font-size: 12.5px;
  padding-bottom: 5px;
  border-bottom: 2px solid var(--blue);
  margin-inline: 12px;
}
.stat .n {
  font-size: 21px;
  font-variant-numeric: tabular-nums;
}
.list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.task {
  padding: 10px 14px 10px 16px;
  display: flex;
  flex-direction: column;
}
.task.queued,
.task.cancelled {
  opacity: 0.8;
}
.tt {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}
.title {
  font-weight: 700;
  font-size: 13.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.grow {
  flex: 1;
}
.row {
  display: grid;
  grid-template-columns: 48px 1fr minmax(120px, auto);
  align-items: center;
  gap: 8px;
  min-height: 28px;
  font-size: 12.5px;
}
.pc {
  color: var(--blue);
  font-variant-numeric: tabular-nums;
}
.row.done .pc {
  color: var(--ok);
}
.row.failed .pc,
.row.failed .name {
  color: var(--bad);
}
.row.waiting {
  color: var(--faint);
}
.row.waiting .pc {
  color: var(--faint);
}
.name {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.bar {
  height: 3px;
  border-radius: 2px;
  background: rgba(19, 112, 243, 0.14);
  overflow: hidden;
  max-width: 320px;
}
.bar i {
  display: block;
  height: 100%;
  background: var(--blue);
  transition: width 0.4s;
}
.detail {
  color: var(--sub);
  font-size: 11.5px;
  text-align: right;
  font-variant-numeric: tabular-nums;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 280px;
}
.hint {
  margin: 8px 0 0;
}
.foot {
  display: flex;
  gap: 14px;
  align-items: center;
  margin-top: 8px;
}
.foot:empty {
  display: none;
}
.logs {
  margin: 8px 0 0;
  max-height: 220px;
  overflow: auto;
  background: rgba(20, 40, 80, 0.05);
  border-radius: 5px;
  padding: 8px 10px;
  font-family: var(--mono);
  font-size: 11.5px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
  user-select: text;
}
</style>
