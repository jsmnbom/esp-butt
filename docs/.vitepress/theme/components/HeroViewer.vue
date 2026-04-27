<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { useData } from "vitepress";
import {
  WebGLRenderer, PerspectiveCamera, Scene, Group,
  Box3, Vector3, AmbientLight, DirectionalLight,
  LinearToneMapping, CanvasTexture, Mesh, MeshBasicMaterial,
  PlaneGeometry, Material, Quaternion, Object3D,
  PMREMGenerator, Timer, Color,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { MeshoptDecoder } from "meshoptimizer/decoder";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import { useRecording, bsearch } from "../composables/useRecording";

const SLIDER_TRAVEL = 0.045;
const CLICK_DIP = 0.003;
const CLICK_HALF_DUR = 0.05;

const props = defineProps<{
  modelUrls: string[];
  height?: number;
}>();

const { isDark } = useData();
const canvasEl = ref<HTMLCanvasElement>();
const containerEl = ref<HTMLDivElement>();
const isHovered = ref(false);
const containerHeight = computed(() => props.height != null ? `${props.height}px` : "100%");

const { recording, duration, sliderEvents, encRotEvents, encSelectTimes, frameEvents } = useRecording();

const NAMED = ["slider_knob1", "slider_knob2", "Lever", "Lever_1", "encoder_knob", "Screen"] as const;
type NamedKey = (typeof NAMED)[number];

function slideInfo(o: Object3D | undefined): { restLocal: Vector3; localDir: Vector3 } | null {
  if (!o) return null;
  o.updateWorldMatrix(true, false);
  const restLocal = o.position.clone();
  const localDir = new Vector3(0, 0, -1);
  if (o.parent) {
    o.parent.updateWorldMatrix(true, false);
    localDir.transformDirection(o.parent.matrixWorld.clone().invert());
  }
  return { restLocal, localDir };
}

let renderer: WebGLRenderer;
let camera: PerspectiveCamera;
let scene: Scene;
let controls: OrbitControls;
let ro: ResizeObserver;

const caseMaterials: Material[] = [];
let sliderTargets: Array<{
  knob: Mesh | undefined; knobInfo: { restLocal: Vector3; localDir: Vector3 } | null;
  lever: Mesh | undefined; leverInfo: { restLocal: Vector3; localDir: Vector3 } | null;
  index: number;
}> = [];
let encObj: Mesh | undefined;
let encRestLocal = new Vector3();
let encRestQuat = new Quaternion();
let encPressLocalDir = new Vector3(0, -1, 0);
let encRotLocalAxis = new Vector3(0, 1, 0);
let screenCanvas: HTMLCanvasElement | null = null;
let canvasTex: CanvasTexture | null = null;
let screenPlaneMesh: Mesh | null = null;

const clearColorDark = new Color("#1b1b1f");
const clearColorLight = new Color("#f0f0f4");

onMounted(async () => {
  const canvas = canvasEl.value!;
  const container = containerEl.value!;

  renderer = new WebGLRenderer({ canvas, antialias: true });
  renderer.toneMapping = LinearToneMapping;
  renderer.toneMappingExposure = 1;
  renderer.setPixelRatio(window.devicePixelRatio);

  camera = new PerspectiveCamera(45, 1, 0.001, 100000);
  scene = new Scene();
  scene.add(new AmbientLight(0xffffff, 0.3));
  const dirLight = new DirectionalLight(0xffffff, 0.8 * Math.PI);
  dirLight.position.set(0.5, 0, 0.866);
  scene.add(dirLight);

  controls = new OrbitControls(camera, canvas);
  controls.enableDamping = false;
  controls.screenSpacePanning = true;

  function resize() {
    const w = container.clientWidth;
    const h = container.clientHeight;
    if (w === 0 || h === 0) return;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }

  ro = new ResizeObserver(resize);
  ro.observe(container);
  resize();

  // Load models
  const loader = new GLTFLoader();
  loader.setMeshoptDecoder(MeshoptDecoder);
  const gltfs = await Promise.all(props.modelUrls.map((url) => loader.loadAsync(url)));

  const root = new Group();
  for (const gltf of gltfs) root.add(gltf.scene);
  root.updateMatrixWorld(true);
  const rootBox = new Box3().setFromObject(root);
  const center = new Vector3();
  rootBox.getCenter(center);
  root.position.sub(center);
  root.updateMatrixWorld(true);

  const modelSize = rootBox.getSize(new Vector3()).length();
  controls.maxDistance = modelSize * 10;
  camera.near = modelSize / 100;
  camera.far = modelSize * 100;
  camera.position.set(modelSize * 0.01, modelSize * 0.8, modelSize * 0.5);
  camera.lookAt(0, 0, 0);
  camera.updateProjectionMatrix();

  const pmrem = new PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment()).texture;
  pmrem.dispose();
  scene.add(root);

  // Ensure correct size now that models are loaded and layout is stable
  resize();

  // Named object lookup
  const obj: Partial<Record<NamedKey, Mesh>> = {};
  root.traverse((node) => {
    if (NAMED.includes(node.name as NamedKey)) obj[node.name as NamedKey] = node as Mesh;
  });

  // Case transparency
  root.traverse((node) => {
    if (!(node as any).isMesh || !["case_top", "case_bottom"].includes(node.name)) return;
    const mesh = node as Mesh;
    const src = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
    const cloned = src.map((m) => { const c = m.clone(); c.transparent = true; c.opacity = 1.0; return c; });
    mesh.material = Array.isArray(mesh.material) ? cloned : cloned[0];
    caseMaterials.push(...cloned);
  });

  sliderTargets = [
    { knob: obj["slider_knob1"], lever: obj["Lever"],   index: 0 },
    { knob: obj["slider_knob2"], lever: obj["Lever_1"], index: 1 },
  ].map(({ knob, lever, index }) => ({ knob, knobInfo: slideInfo(knob), lever, leverInfo: slideInfo(lever), index }));

  encObj = obj["encoder_knob"];
  if (encObj) {
    encRestLocal = encObj.position.clone();
    encRestQuat = encObj.quaternion.clone();
    if (encObj.parent) {
      encObj.parent.updateWorldMatrix(true, false);
      const inv = encObj.parent.matrixWorld.clone().invert();
      encPressLocalDir = new Vector3(0, -1, 0).transformDirection(inv.clone());
      encRotLocalAxis = new Vector3(0, 1, 0).transformDirection(inv).normalize();
    }
  }

  const screenMesh = obj["Screen"];
  if (screenMesh) {
    screenMesh.updateWorldMatrix(true, false);
    const sbox = new Box3().setFromObject(screenMesh);
    const sw = sbox.max.x - sbox.min.x;
    const sd = sbox.max.z - sbox.min.z;
    screenCanvas = document.createElement("canvas");
    screenCanvas.width = 128; screenCanvas.height = 64;
    canvasTex = new CanvasTexture(screenCanvas);
    const ctx = screenCanvas.getContext("2d")!;
    ctx.fillStyle = "#000"; ctx.fillRect(0, 0, 128, 64);
    screenPlaneMesh = new Mesh(new PlaneGeometry(sw, sd), new MeshBasicMaterial({ map: canvasTex }));
    screenPlaneMesh.rotation.x = -Math.PI / 2;
    screenPlaneMesh.position.set(
      (sbox.min.x + sbox.max.x) / 2,
      sbox.max.y + 0.001,
      (sbox.min.z + sbox.max.z) / 2,
    );
    scene.add(screenPlaneMesh);
  }

  function updateClearColor() {
    renderer.setClearColor(isDark.value ? clearColorDark : clearColorLight);
  }
  updateClearColor();
  watch(isDark, updateClearColor);

  const timer = new Timer();
  renderer.setAnimationLoop(() => {
    timer.update();
    const delta = timer.getDelta();
    const elapsed = timer.getElapsed();
    const pt = elapsed % duration;

    const targetOpacity = isHovered.value ? 0.35 : 1.0;
    for (const mat of caseMaterials) {
      mat.opacity += (targetOpacity - mat.opacity) * Math.min(1, delta * 6);
    }

    for (const { knob, knobInfo, lever, leverInfo, index } of sliderTargets) {
      const fi = bsearch(sliderEvents[index], pt);
      const value = fi >= 0 ? sliderEvents[index][fi].value : 0;
      const dy = (value / 4095) * SLIDER_TRAVEL;
      if (knob  && knobInfo)  knob.position.copy(knobInfo.restLocal).addScaledVector(knobInfo.localDir, dy);
      if (lever && leverInfo) lever.position.copy(leverInfo.restLocal).addScaledVector(leverInfo.localDir, dy);
    }

    if (encObj) {
      const ri = bsearch(encRotEvents, pt);
      const angle = ri >= 0 ? encRotEvents[ri].cumAngle : 0;
      const spinQuat = new Quaternion().setFromAxisAngle(encRotLocalAxis, (angle * Math.PI) / 180);
      encObj.quaternion.copy(encRestQuat).multiply(spinQuat);

      const si = bsearch(encSelectTimes, pt);
      let dip = 0;
      if (si >= 0) {
        const dt = pt - encSelectTimes[si].t;
        const dipDur = CLICK_HALF_DUR * 2;
        if (dt < dipDur) {
          dip = dt < CLICK_HALF_DUR ? dt / CLICK_HALF_DUR : 1 - (dt - CLICK_HALF_DUR) / CLICK_HALF_DUR;
        }
      }
      encObj.position.copy(encRestLocal).addScaledVector(encPressLocalDir, dip * CLICK_DIP);
    }

    const frames = recording.value?.frames ?? [];
    if (screenCanvas && canvasTex && frames.length > 0 && frameEvents.length > 0) {
      const fi = bsearch(frameEvents, pt);
      if (fi >= 0) {
        const bitmap = frames[frameEvents[fi].frame];
        if (bitmap) {
          screenCanvas.getContext("2d")!.drawImage(bitmap, 0, 0, screenCanvas.width, screenCanvas.height);
          canvasTex.needsUpdate = true;
        }
      }
    }

    controls.update();
    renderer.render(scene, camera);
  });
});

onUnmounted(() => {
  renderer?.setAnimationLoop(null);
  controls?.dispose();
  ro?.disconnect();
  canvasTex?.dispose();
  if (screenPlaneMesh) {
    scene?.remove(screenPlaneMesh);
    (screenPlaneMesh.material as MeshBasicMaterial)?.dispose();
    screenPlaneMesh.geometry.dispose();
  }
  renderer?.dispose();
});
</script>

<template>
  <div ref="containerEl" class="hero-viewer-host" :class="{ 'is-hovered': isHovered }"
    :style="{ height: containerHeight }"
    @mouseenter="isHovered = true" @mouseleave="isHovered = false">
    <canvas ref="canvasEl" class="hero-canvas" />
    <div class="hero-viewer-hint" :class="{ visible: isHovered }">Scroll to zoom · drag to rotate</div>
  </div>
</template>

<style scoped>
.hero-viewer-host {
  width: 100%;
  height: 100%;
  border-radius: 10px;
  overflow: hidden;
  position: relative;
  transition: box-shadow 0.2s ease;
}

.hero-viewer-host.is-hovered {
  box-shadow: 0 0 0 2px var(--vp-c-brand-1);
}

.hero-canvas {
  width: 100%;
  height: 100%;
  display: block;
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
