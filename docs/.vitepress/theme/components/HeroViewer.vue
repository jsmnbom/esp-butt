<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { useData } from "vitepress";
import {
  AmbientLight,
  Box3,
  CanvasTexture,
  Color,
  DirectionalLight,
  Group,
  Mesh,
  MeshBasicMaterial,
  Object3D,
  PerspectiveCamera,
  PlaneGeometry,
  PMREMGenerator,
  Quaternion,
  Scene,
  Vector3,
  WebGLRenderer,
} from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

const props = defineProps<{
  primaryUrl: string;
  secondaryUrl: string;
  recordingUrl: string;
  height?: number;
}>();

const { isDark } = useData();

const containerRef = ref<HTMLDivElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);
const containerHeight = computed(() => `${props.height ?? 500}px`);

const PADDING = 0.5;          // seconds of rest pose before/after recording
const SLIDER_TRAVEL = 0.045;
const DEG_PER_CLICK = 15;
const CLICK_DIP = 0.003;      // world units
const CLICK_HALF_DUR = 0.05;  // half-duration of encoder press dip (seconds)
const CLEAR_DARK = "#212128";
const CLEAR_LIGHT = "#f0f0f4";

let renderer: WebGLRenderer | null = null;
let orbitControls: OrbitControls | null = null;
let animFrameId = 0;

watch(isDark, (dark) => {
  renderer?.setClearColor(new Color(dark ? CLEAR_DARK : CLEAR_LIGHT));
});

onMounted(async () => {
  const canvas = canvasRef.value!;
  const container = containerRef.value!;

  renderer = new WebGLRenderer({ canvas, antialias: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  renderer.setSize(container.clientWidth, container.clientHeight);
  renderer.setClearColor(new Color(isDark.value ? CLEAR_DARK : CLEAR_LIGHT));

  const scene = new Scene();

  const camera = new PerspectiveCamera(
    45,
    container.clientWidth / container.clientHeight,
    0.001,
    100000
  );

  // Lights attached to camera so they always face the model
  const ambient = new AmbientLight(0xffffff, 0.3);
  const dirLight = new DirectionalLight(0xffffff, 0.8 * Math.PI);
  dirLight.position.set(0.5, 0, 0.866);
  camera.add(ambient, dirLight);
  scene.add(camera);

  // IBL from neutral room environment
  const pmrem = new PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment()).texture;
  pmrem.dispose();

  orbitControls = new OrbitControls(camera, renderer.domElement);
  orbitControls.enableDamping = false;
  orbitControls.screenSpacePanning = true;

  // Load both GLBs + recording in parallel
  const loader = new GLTFLoader();
  const atlasUrl = props.recordingUrl.replace(/[^/]*$/, "screen-atlas.png");

  const [gltf1, gltf2, recording] = await Promise.all([
    loader.loadAsync(props.primaryUrl),
    loader.loadAsync(props.secondaryUrl),
    fetch(props.recordingUrl).then((r) => r.json() as Promise<Recording>).catch(() => ({ events: [] } as Recording)),
  ]);

  // Pre-load atlas image (used in render loop)
  const atlasImg = new Image();
  atlasImg.crossOrigin = "anonymous";
  atlasImg.src = atlasUrl;

  // Merge both scenes under a single root group
  const root = new Group();
  root.add(gltf1.scene, gltf2.scene);
  scene.add(root);

  // Center model at origin
  root.updateMatrixWorld(true);
  const rootBox = new Box3().setFromObject(root);
  const center = new Vector3();
  rootBox.getCenter(center);
  root.position.sub(center);
  root.updateMatrixWorld(true);

  const modelSize = rootBox.getSize(new Vector3()).length();
  camera.near = modelSize / 100;
  camera.far = modelSize * 100;
  camera.position.set(0, modelSize * 0.75, modelSize);
  camera.lookAt(0, 0, 0);
  camera.updateProjectionMatrix();
  orbitControls.maxDistance = modelSize * 10;

  // Find named objects by traversal — log all names to help identify correct names
  const NAMED = [
    "slider_knob1",
    "slider_knob2",
    "Lever",
    "Lever_1",
    "encoder_knob",
    "Screen",
  ] as const;
  type NamedKey = (typeof NAMED)[number];
  const obj: Partial<Record<NamedKey, Mesh>> = {};
  const allNames: string[] = [];
  root.traverse((node) => {
    if (node.name) allNames.push(node.name);
    if (NAMED.includes(node.name as NamedKey)) {
      obj[node.name as NamedKey] = node as Mesh;
    }
  });
  console.log("[HeroViewer] all object names:", allNames);
  console.log("[HeroViewer] found named objects:", Object.keys(obj));

  // ── Slider + encoder helpers (geometry, confirmed working) ──────────────────

  // Returns rest local position and the slide direction in local space.
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

  const sliderTargets = [
    { knob: obj["slider_knob1"], lever: obj["Lever"],   index: 0 },
    { knob: obj["slider_knob2"], lever: obj["Lever_1"], index: 1 },
  ].map(({ knob, lever, index }) => ({
    knob,  knobInfo:  slideInfo(knob),
    lever, leverInfo: slideInfo(lever),
    index,
  }));

  function driveObject(o: Object3D, info: { restLocal: Vector3; localDir: Vector3 }, dy: number) {
    o.position.copy(info.restLocal).addScaledVector(info.localDir, dy);
  }

  // ── Encoder animation ─────────────────────────────────────────────────────
  const encObj = obj["encoder_knob"];
  // World -Z is the slider rail axis; world Z is the encoder press axis (into the board).
  // Rotation is around world Z (into the board from above).
  const encRestLocal  = encObj ? encObj.position.clone() : new Vector3();
  const encRestQuat   = encObj ? encObj.quaternion.clone() : new Quaternion();

  // Precompute world-Z as a local direction for the press dip.
  const encPressLocalDir = new Vector3(0, -1, 0);
  if (encObj?.parent) {
    encObj.parent.updateWorldMatrix(true, false);
    encPressLocalDir.transformDirection(encObj.parent.matrixWorld.clone().invert());
  }

  // Precompute world-Z as a local rotation axis for the spin.
  const encRotLocalAxis = new Vector3(0, 1, 0);
  if (encObj?.parent) {
    encObj.parent.updateWorldMatrix(true, false);
    encRotLocalAxis.transformDirection(encObj.parent.matrixWorld.clone().invert()).normalize();
  }

  // ── Preprocess recording events ────────────────────────────────────────────
  // All times are offset by PADDING so the first event fires after a pause.
  const events = recording.events;
  const lastT = events.length > 0 ? Math.max(...events.map((e) => e.t)) : 0;
  const duration = lastT + 2 * PADDING;

  // Per-slider: sorted [{t, value}] arrays (index 0 and 1)
  const sliderEvents: Array<Array<{ t: number; value: number }>> = [[], []];
  // Encoder rotation: pre-accumulated angle
  const encRotEvents: Array<{ t: number; cumAngle: number }> = [];
  // Encoder select times
  const encSelectTimes: number[] = [];
  // Frame events: [{t, col, row}]
  const frameEvents: Array<{ t: number; col: number; row: number }> = [];

  let cumAngleDeg = 0;
  for (const e of events) {
    const t = e.t + PADDING;
    if (e.type === "slider" && e.index !== undefined && e.value !== undefined) {
      sliderEvents[e.index]?.push({ t, value: e.value });
    } else if (e.type === "nav") {
      if (e.event === "Up") {
        cumAngleDeg += DEG_PER_CLICK;
        encRotEvents.push({ t, cumAngle: cumAngleDeg });
      } else if (e.event === "Down") {
        cumAngleDeg -= DEG_PER_CLICK;
        encRotEvents.push({ t, cumAngle: cumAngleDeg });
      } else if (e.event === "Select") {
        encSelectTimes.push(t);
      }
    } else if (e.type === "frame" && e.col !== undefined && e.row !== undefined) {
      frameEvents.push({ t, col: e.col, row: e.row });
    }
  }

  // ── Screen plane ──────────────────────────────────────────────────
  const screenMesh = obj["Screen"];
  let screenCanvas: HTMLCanvasElement | null = null;
  let canvasTex: CanvasTexture | null = null;

  if (screenMesh) {
    screenMesh.updateWorldMatrix(true, false);
    const sbox = new Box3().setFromObject(screenMesh);
    // Screen lies flat in XZ. Dimensions: X width, Z depth; surface at sbox.max.y.
    const sw  = sbox.max.x - sbox.min.x;
    const sd  = sbox.max.z - sbox.min.z;  // Z extent = slide depth
    const scx = (sbox.min.x + sbox.max.x) / 2;
    const scy = sbox.max.y + 0.001;        // 1mm above the surface
    const scz = (sbox.min.z + sbox.max.z) / 2;
    console.log("[HeroViewer] Screen bbox:", sbox, "plane size:", sw, "×", sd, "at y:", scy);

    screenCanvas = document.createElement("canvas");
    screenCanvas.width = 128;
    screenCanvas.height = 64;
    canvasTex = new CanvasTexture(screenCanvas);

    // Draw black until atlas loads
    const ctx = screenCanvas.getContext("2d")!;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, 128, 64);

    const planeMesh = new Mesh(
      new PlaneGeometry(sw, sd),
      new MeshBasicMaterial({ map: canvasTex })
    );
    // PlaneGeometry is in XY by default; rotate -90° around X to lie flat in XZ, facing up.
    planeMesh.rotation.x = -Math.PI / 2;
    planeMesh.position.set(scx, scy, scz);
    scene.add(planeMesh);
  }

  // ── Render loop ───────────────────────────────────────────────────────────
  let elapsed = 0;
  let prevTime = performance.now() / 1000;

  // Binary search: index of last entry with entry.t <= t, or -1 if none.
  function bsearch<T extends { t: number }>(arr: T[], t: number): number {
    let lo = 0, hi = arr.length - 1, fi = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (arr[mid].t <= t) { fi = mid; lo = mid + 1; }
      else hi = mid - 1;
    }
    return fi;
  }

  function animate() {
    animFrameId = requestAnimationFrame(animate);
    const now = performance.now() / 1000;
    const delta = now - prevTime;
    prevTime = now;
    elapsed += delta;

    orbitControls?.update();

    const pt = elapsed % duration; // playback time within one loop

    // ── Sliders ──────────────────────────────────────────────────────────────
    for (const { knob, knobInfo, lever, leverInfo, index } of sliderTargets) {
      const fi = bsearch(sliderEvents[index], pt);
      const value = fi >= 0 ? sliderEvents[index][fi].value : 0;
      const dy = (value / 4095) * SLIDER_TRAVEL;
      if (knob  && knobInfo)  driveObject(knob,  knobInfo,  dy);
      if (lever && leverInfo) driveObject(lever, leverInfo, dy);
    }

    // ── Encoder rotation ──────────────────────────────────────────────────────
    if (encObj) {
      const ri = bsearch(encRotEvents, pt);
      const angle = ri >= 0 ? encRotEvents[ri].cumAngle : 0;
      const spinQuat = new Quaternion().setFromAxisAngle(encRotLocalAxis, (angle * Math.PI) / 180);
      encObj.quaternion.copy(encRestQuat).multiply(spinQuat);

      // Encoder select dip
      const encSelectObjs = encSelectTimes.map((t) => ({ t }));
      const si = bsearch(encSelectObjs, pt);
      let dip = 0;
      if (si >= 0) {
        const dt = pt - encSelectTimes[si];
        const dipDur = CLICK_HALF_DUR * 2;
        if (dt < dipDur) {
          dip = dt < CLICK_HALF_DUR
            ? dt / CLICK_HALF_DUR
            : 1 - (dt - CLICK_HALF_DUR) / CLICK_HALF_DUR;
        }
      }
      encObj.position.copy(encRestLocal).addScaledVector(encPressLocalDir, dip * CLICK_DIP);
    }

    // ── Screen frame ──────────────────────────────────────────────────────────
    if (screenCanvas && canvasTex && atlasImg.complete && frameEvents.length > 0) {
      const fi = bsearch(frameEvents, pt);
      if (fi >= 0) {
        const { col, row } = frameEvents[fi];
        const ctx = screenCanvas.getContext("2d")!;
        ctx.drawImage(atlasImg, col, row, 128, 64, 0, 0, 128, 64);
        canvasTex.needsUpdate = true;
      }
    }

    renderer!.render(scene, camera);
  }

  animate();
});

onUnmounted(() => {
  cancelAnimationFrame(animFrameId);
  orbitControls?.dispose();
  renderer?.dispose();
});

interface RecordingEvent {
  t: number;
  type: "frame" | "slider" | "nav";
  col?: number;
  row?: number;
  index?: number;
  value?: number;
  event?: string;
}
interface Recording {
  events: RecordingEvent[];
}
</script>

<template>
  <div
    ref="containerRef"
    class="hero-viewer-host"
    :style="{ height: containerHeight }"
  >
    <canvas ref="canvasRef" class="hero-viewer-canvas" />
    <div class="hero-viewer-hint">Scroll to zoom · drag to rotate</div>
  </div>
</template>

<style scoped>
.hero-viewer-host {
  width: 100%;
  border-radius: 10px;
  overflow: hidden;
  position: relative;
  box-shadow: 0 0 0 1px var(--vp-c-divider);
}

.hero-viewer-canvas {
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
  opacity: 0.6;
  white-space: nowrap;
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}
</style>
