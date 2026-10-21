<script setup lang="ts">
// 可折叠卡片：标题 + 数量，点标题展开/收起。
import { ref, watch } from "vue";

const props = withDefaults(defineProps<{ title: string; count?: string | number; open?: boolean; foldable?: boolean }>(), {
  open: true,
  foldable: true,
});
const isOpen = ref(props.open);
watch(
  () => props.open,
  (v) => (isOpen.value = v),
);
const emit = defineEmits<{ toggle: [boolean] }>();
function toggle() {
  if (!props.foldable) return;
  isOpen.value = !isOpen.value;
  emit("toggle", isOpen.value);
}
</script>

<template>
  <div class="card fold" :class="{ closed: !isOpen }">
    <component :is="foldable ? 'button' : 'div'" class="card-h" type="button" @click="toggle">
      {{ title }}
      <span v-if="count !== undefined" class="cnt">({{ count }})</span>
      <slot name="head" />
      <span v-if="foldable" class="chev">›</span>
    </component>
    <div v-if="isOpen" class="card-b"><slot /></div>
  </div>
</template>

<style scoped>
.chev {
  margin-left: auto;
  color: var(--sub);
  font-size: 17px;
  line-height: 1;
  transition: transform 0.2s;
  transform: rotate(90deg);
}
.closed .chev {
  transform: rotate(0deg);
}
</style>
