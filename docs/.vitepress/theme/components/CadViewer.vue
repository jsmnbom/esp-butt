<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { useData } from "vitepress";
import {
  WebGLRenderer, PerspectiveCamera, Scene,
  Box3, Vector3, LinearToneMapping, PMREMGenerator, Color,
  EdgesGeometry, LineBasicMaterial, LineSegments,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { MeshoptDecoder } from "meshoptimizer/decoder";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

const props = defineProps<{
  url: string;
  height?: number;
}>();

const { isDark } = useData();
const canvasEl = ref<HTMLCanvasElement>();
const containerEl = ref<HTMLDivElement>();
const isHovered = ref(false);
const containerHeight = computed(() => `${props.height ?? 400}px`);

const clearColorDark = new Color("#212128");
const clearColorLight = new Color("#f0f0f4");

let renderer: WebGLRenderer;
let camera: PerspectiveCamera;
let scene: Scene;
let controls: OrbitControls;
let ro: ResizeObserver;

onMounted(async () => {
  const canvas = canvasEl.value!;
  const container = containerEl.value!;

  renderer = new WebGLRenderer({ canvas, antialias: true });
  renderer.toneMapping = LinearToneMapping;
  renderer.toneMappingExposure = 1;
  renderer.shadowMap.enabled = true;
  renderer.setPixelRatio(window.devicePixelRatio);

  camera = new PerspectiveCamera(45, 1, 0.001, 100000);
  scene = new Scene();

  controls = new OrbitControls(camera, canvas);
  controls.enableDamping = false;
  controls.screenSpacePanning = true;

  ro = new ResizeObserver(() => {
    const w = container.clientWidth;
    const h = container.clientHeight;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  });
  ro.observe(container);
  {
    const w = container.clientWidth; const h = container.clientHeight;
    renderer.setSize(w, h, false); camera.aspect = w / h; camera.updateProjectionMatrix();
  }

  const loader = new GLTFLoader();
  loader.setMeshoptDecoder(MeshoptDecoder);
  const gltf = await loader.loadAsync(props.url);
  const gltfScene = gltf.scene;

  gltfScene.updateMatrixWorld();
  const box = new Box3().setFromObject(gltfScene);
  const center = new Vector3();
  const sizeVec = new Vector3();
  box.getCenter(center);
  box.getSize(sizeVec);
  const modelSize = sizeVec.length();

  // Center model at origin
  gltfScene.position.sub(center);

  const pmrem = new PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment()).texture;
  pmrem.dispose();

  const edgeMaterial = new LineBasicMaterial({ color: 0x000000, opacity: 0.25, transparent: true });
  gltfScene.traverse((node: any) => {
    if (!node.isMesh) return;
    const edges = new EdgesGeometry(node.geometry, 20);
    const lines = new LineSegments(edges, edgeMaterial);
    lines.raycast = () => { };
    node.add(lines);
  });
  scene.add(gltfScene);

  // Fit camera to bounding box
  controls.maxDistance = modelSize * 10;
  camera.near = modelSize / 100;
  camera.far = modelSize * 100;
  camera.updateProjectionMatrix();
  const fov = camera.fov * (Math.PI / 180);
  const aspect = container.clientWidth / container.clientHeight;
  const fovH = 2 * Math.atan(Math.tan(fov / 2) * aspect);
  const fitDist = Math.max(
    (modelSize / 2) / Math.tan(fovH / 2),
    (modelSize / 2) / Math.tan(fov / 2),
  ) * 0.8;
  camera.position.set(fitDist * 0.5, fitDist * 0.4, fitDist);
  camera.lookAt(0, 0, 0);
  controls.update();

  function updateClearColor() {
    renderer.setClearColor(isDark.value ? clearColorDark : clearColorLight);
  }
  updateClearColor();
  watch(isDark, updateClearColor);

  renderer.setAnimationLoop(() => {
    controls.update();
    renderer.render(scene, camera);
  });
});

onUnmounted(() => {
  renderer?.setAnimationLoop(null);
  controls?.dispose();
  ro?.disconnect();
  renderer?.dispose();
});
</script>

<template>
  <div ref="containerEl" class="cad-viewer-host" :class="{ 'is-hovered': isHovered }"
    :style="{ height: containerHeight }" @mouseenter="isHovered = true" @mouseleave="isHovered = false">
    <canvas ref="canvasEl" class="cad-canvas" />
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

.cad-canvas {
  width: 100%;
  height: 100%;
  display: block;
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
