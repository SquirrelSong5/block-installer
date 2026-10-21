<script setup lang="ts">
// 8×8 像素方块图标，用画布生成，不需要图片资源。
import { computed } from "vue";

const props = withDefaults(defineProps<{ kind?: string; size?: number }>(), { kind: "grass", size: 28 });

type Pal = { top?: string[]; body?: string[]; all?: string[] };
const PAL: Record<string, Pal> = {
  grass: { top: ["#6cb544", "#5ea13a", "#7cc451", "#4f8f30"], body: ["#8b5a33", "#7a4d2b", "#9a6a40", "#6b4226"] },
  cobble: { all: ["#8d8d8d", "#7a7a7a", "#a2a2a2", "#666666"] },
  cmd: { all: ["#c97f3a", "#b46d2c", "#d99350", "#8f5522"] },
  cake: { top: ["#f4f1ee", "#e9e4df", "#ffffff", "#dcd5ce"], body: ["#b5653a", "#c97a4a", "#e24b4b", "#9e5530"] },
  forge: { all: ["#3d4148", "#4b5058", "#5c626b", "#2f3237"] },
  neoForge: { all: ["#e8892e", "#d97a22", "#f29b43", "#c36a18"] },
  fabric: { all: ["#dbd0b4", "#cdbf9e", "#e6dcc4", "#bfae89"] },
  fabricApi: { all: ["#c9b98f", "#dbd0b4", "#9fc4e8", "#bfae89"] },
  quilt: { all: ["#9b6bd6", "#7d55c0", "#4f9bd9", "#b98ae6"] },
  optiFine: { all: ["#2f6fd6", "#c33d3d", "#e9edf4", "#255bb3"] },
  java: { all: ["#e76f00", "#5382a1", "#f89820", "#3a6a8c"] },
  folder: { top: ["#f2c14e", "#e8b23a", "#f6cf6a", "#d9a12b"], body: ["#e8b23a", "#d9a12b", "#f2c14e", "#c99222"] },
};

const cache = new Map<string, string>();
function texture(kind: string): string {
  const hit = cache.get(kind);
  if (hit !== undefined) return hit;
  const p = PAL[kind] ?? PAL.grass;
  const c = document.createElement("canvas");
  c.width = c.height = 8;
  const g = c.getContext("2d");
  let url = "";
  if (g) {
    let seed = kind.length * 97 + kind.charCodeAt(0);
    const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
    for (let y = 0; y < 8; y++)
      for (let x = 0; x < 8; x++) {
        const set = p.all ?? (y < 2 || (y === 2 && rnd() < 0.5) ? p.top! : p.body!);
        g.fillStyle = set[Math.floor(rnd() * set.length)];
        g.fillRect(x, y, 1, 1);
      }
    url = c.toDataURL();
  }
  cache.set(kind, url);
  return url;
}

const style = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  backgroundImage: `url(${texture(props.kind)})`,
}));
</script>

<template>
  <span class="px" :style="style" aria-hidden="true"></span>
</template>

<style scoped>
.px {
  display: inline-block;
  flex: none;
  border-radius: 2px;
  image-rendering: pixelated;
  background-size: 100% 100%;
  background-color: #8a8f99;
}
</style>
