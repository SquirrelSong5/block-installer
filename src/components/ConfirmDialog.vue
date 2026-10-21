<script setup lang="ts">
defineProps<{ title: string; message: string; confirmText?: string; danger?: boolean }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <div class="mask" @click.self="emit('cancel')">
    <div class="dialog card" role="dialog" aria-modal="true">
      <h3>{{ title }}</h3>
      <p>{{ message }}</p>
      <div class="acts">
        <button class="btn" type="button" @click="emit('cancel')">取消</button>
        <button class="btn" :class="danger ? 'danger' : 'primary'" type="button" @click="emit('confirm')">
          {{ confirmText ?? "确定" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 48px 0 0;
  background: rgba(30, 50, 90, 0.25);
  display: grid;
  place-items: center;
  z-index: 50;
}
.dialog {
  width: 380px;
  padding: 18px 20px 16px;
  background: #fff;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
h3 {
  margin: 0;
  font-size: 15px;
}
p {
  margin: 0;
  color: #5d6676;
  line-height: 1.6;
  white-space: pre-line;
}
.acts {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 6px;
}
</style>
