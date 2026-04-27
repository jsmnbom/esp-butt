<script setup lang="ts">
import { ref, computed } from "vue";
import { useData } from "vitepress";
import { TresCanvas } from "@tresjs/core";
import HeroScene from "./HeroScene.vue";

const props = defineProps<{
  primaryUrl: string;
  secondaryUrl: string;
  recordingUrl: string;
  height?: number;
}>();

const { isDark } = useData();

const clearColor = computed(() => isDark.value ? "#212128" : "#f0f0f4");
const containerHeight = computed(() => `${props.height ?? 500}px`);
const isHovered = ref(false);

</script>

<template>
  <div
    class="hero-viewer-host"
    :class="{ 'is-hovered': isHovered }"
    :style="{ height: containerHeight }"
    @mouseenter="isHovered = true"
    @mouseleave="isHovered = false"
  >
    <!-- tone-mapping="1" = THREE.LinearToneMapping (matches three-gltf-viewer default) -->
    <TresCanvas :clear-color="clearColor" :window-size="false" :tone-mapping="1" :tone-mapping-exposure="1">
      <TresPerspectiveCamera :position="[1, 1, 1]" :look-at="[0, 0, 0]" :near="0.001" :far="100000">
        <TresAmbientLight :intensity="0.3" />
        <TresDirectionalLight :intensity="0.8 * Math.PI" :position="[0.5, 0, 0.866]" />
      </TresPerspectiveCamera>
      <Suspense>
        <HeroScene
          :primary-url="primaryUrl"
          :secondary-url="secondaryUrl"
          :recording-url="recordingUrl"
          :hovered="isHovered"
        />
      </Suspense>
    </TresCanvas>
    <div class="hero-viewer-hint" :class="{ visible: isHovered }">Scroll to zoom · drag to rotate</div>
  </div>
</template>

<style scoped>
.hero-viewer-host {
  width: 100%;
  border-radius: 10px;
  overflow: hidden;
  position: relative;
  box-shadow: 0 0 0 1px var(--vp-c-divider);
  transition: box-shadow 0.2s ease;
}

.hero-viewer-host.is-hovered {
  box-shadow: 0 0 0 2px var(--vp-c-brand-1);
}

.hero-viewer-hint {
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

.hero-viewer-hint.visible {
  opacity: 1;
}
</style>
