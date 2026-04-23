<script setup lang="ts">
import { computed, ref } from "vue";
import { useData } from "vitepress";
import { TresCanvas } from "@tresjs/core";
import CadScene from "./CadScene.vue";

const props = defineProps<{
  url: string;
  height?: number;
}>();

const { isDark } = useData();

// Slightly off the page background so the viewer has its own visual panel
const clearColor = computed(() => isDark.value ? "#212128" : "#f0f0f4");
const containerHeight = computed(() => `${props.height ?? 400}px`);

const isHovered = ref(false);
</script>

<template>
  <div
    class="cad-viewer-host"
    :class="{ 'is-hovered': isHovered }"
    :style="{ height: containerHeight }"
    @mouseenter="isHovered = true"
    @mouseleave="isHovered = false"
  >
    <!-- tone-mapping="1" = THREE.LinearToneMapping (matches three-gltf-viewer default) -->
    <TresCanvas :clear-color="clearColor" :window-size="false" :tone-mapping="1" :tone-mapping-exposure="1" :shadows="true">
      <TresPerspectiveCamera :position="[1, 1, 1]" :look-at="[0, 0, 0]" :near="0.001" :far="100000" />
      <Suspense>
        <CadScene :url="url" />
      </Suspense>
    </TresCanvas>
    <div class="cad-viewer-hint" :class="{ visible: isHovered }">
      Scroll to zoom · drag to rotate
    </div>
  </div>
</template>

<style scoped>
.cad-viewer-host {
  width: 100%;
  border-radius: 10px;
  overflow: hidden;
  position: relative;
  box-shadow: 0 0 0 1px var(--vp-c-divider);
  transition: box-shadow 0.2s ease;
}

.cad-viewer-host.is-hovered {
  box-shadow: 0 0 0 2px var(--vp-c-brand-1);
}

.cad-viewer-hint {
  position: absolute;
  bottom: 10px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(0, 0, 0, 0.5);
  color: #fff;
  font-size: 12px;
  padding: 4px 12px;
  border-radius: 20px;
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.15s ease 0.4s;
  white-space: nowrap;
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.cad-viewer-hint.visible {
  opacity: 1;
}
</style>
