<script setup lang="ts">
import { shallowRef, onUnmounted } from "vue";
import { useTresContext, useLoop } from "@tresjs/core";
import { OrbitControls } from "@tresjs/cientos";
import {
  Box3,
  CanvasTexture,
  Group,
  Material,
  Mesh,
  MeshBasicMaterial,
  Object3D,
  PlaneGeometry,
  PMREMGenerator,
  Quaternion,
  Vector3,
  WebGLRenderer,
} from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import { useGifFrames } from "../composables/useGifFrames";
import { useRecording, bsearch, type Recording } from "../composables/useRecording";

const SLIDER_TRAVEL = 0.045;
const CLICK_DIP = 0.003;      // world units
const CLICK_HALF_DUR = 0.05;  // seconds

const props = defineProps<{
  primaryUrl: string;
  secondaryUrl: string;
  recordingUrl: string;
  hovered?: boolean;
}>();

// inject calls must happen before any await
const { camera, scene, renderer } = useTresContext();

const gifUrl = props.recordingUrl.replace(/[^/]*$/, "session.gif");

const [gltf1, gltf2, recording, gifBitmaps] = await Promise.all([
  new GLTFLoader().loadAsync(props.primaryUrl),
  new GLTFLoader().loadAsync(props.secondaryUrl),
  fetch(props.recordingUrl)
    .then((r) => r.json() as Promise<Recording>)
    .catch(() => ({ events: [] }) as Recording),
  useGifFrames(gifUrl).catch((err) => {
    console.warn("[HeroScene] GIF decode failed:", err);
    return [] as ImageBitmap[];
  }),
]);

// ── Scene setup ───────────────────────────────────────────────────────────────

const root = new Group();
root.add(gltf1.scene, gltf2.scene);

root.updateMatrixWorld(true);
const rootBox = new Box3().setFromObject(root);
const center = new Vector3();
rootBox.getCenter(center);
root.position.sub(center);
root.updateMatrixWorld(true);

const modelSize = rootBox.getSize(new Vector3()).length();
const maxDistance = modelSize * 10;

// Camera: near/far and initial position
const activeCam = camera.activeCamera.value;
if (activeCam && "near" in activeCam) {
  activeCam.near = modelSize / 100;
  activeCam.far = modelSize * 100;
  (activeCam as any).updateProjectionMatrix?.();
  activeCam.position.set(0, modelSize * 0.75, modelSize);
  activeCam.lookAt(0, 0, 0);
}

// IBL from neutral room environment
const pmrem = new PMREMGenerator(renderer.instance as WebGLRenderer);
scene.value.environment = pmrem.fromScene(new RoomEnvironment()).texture;
pmrem.dispose();

// ── Named object lookup ───────────────────────────────────────────────────────

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
root.traverse((node) => {
  if (NAMED.includes(node.name as NamedKey)) {
    obj[node.name as NamedKey] = node as Mesh;
  }
});

// ── Case transparency on hover ────────────────────────────────────────────────
const CASE_NAMES = new Set(["case_top", "case_bottom"]);
const caseMaterials: Material[] = [];
root.traverse((node) => {
  if (!(node as any).isMesh || !CASE_NAMES.has(node.name)) return;
  const mesh = node as Mesh;
  const src = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
  const cloned = src.map((m) => {
    const c = m.clone();
    c.transparent = true;
    c.opacity = 1.0;
    return c;
  });
  mesh.material = Array.isArray(mesh.material) ? cloned : cloned[0];
  caseMaterials.push(...cloned);
});

// ── Slider animation ──────────────────────────────────────────────────────────

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

// ── Encoder animation ─────────────────────────────────────────────────────────

const encObj = obj["encoder_knob"];
const encRestLocal = encObj ? encObj.position.clone() : new Vector3();
const encRestQuat  = encObj ? encObj.quaternion.clone() : new Quaternion();

const encPressLocalDir = new Vector3(0, -1, 0);
const encRotLocalAxis  = new Vector3(0, 1, 0);
if (encObj?.parent) {
  encObj.parent.updateWorldMatrix(true, false);
  const inv = encObj.parent.matrixWorld.clone().invert();
  encPressLocalDir.transformDirection(inv.clone());
  encRotLocalAxis.transformDirection(inv).normalize();
}

// ── Screen plane ──────────────────────────────────────────────────────────────

const screenPlaneMesh = shallowRef<Mesh | null>(null);
let screenCanvas: HTMLCanvasElement | null = null;
let canvasTex: CanvasTexture | null = null;

const screenMesh = obj["Screen"];
if (screenMesh) {
  screenMesh.updateWorldMatrix(true, false);
  const sbox = new Box3().setFromObject(screenMesh);
  const sw  = sbox.max.x - sbox.min.x;
  const sd  = sbox.max.z - sbox.min.z;
  const scx = (sbox.min.x + sbox.max.x) / 2;
  const scy = sbox.max.y + 0.001; // 1 mm above surface
  const scz = (sbox.min.z + sbox.max.z) / 2;

  screenCanvas = document.createElement("canvas");
  screenCanvas.width = 128;
  screenCanvas.height = 64;
  canvasTex = new CanvasTexture(screenCanvas);

  const ctx = screenCanvas.getContext("2d")!;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, 128, 64);

  const plane = new Mesh(
    new PlaneGeometry(sw, sd),
    new MeshBasicMaterial({ map: canvasTex }),
  );
  // PlaneGeometry is XY; rotate to lie flat in XZ facing up.
  plane.rotation.x = -Math.PI / 2;
  plane.position.set(scx, scy, scz);
  screenPlaneMesh.value = plane;
}

onUnmounted(() => {
  canvasTex?.dispose();
  (screenPlaneMesh.value?.material as MeshBasicMaterial | undefined)?.dispose();
  screenPlaneMesh.value?.geometry.dispose();
});

// ── Recording ─────────────────────────────────────────────────────────────────

const { duration, sliderEvents, encRotEvents, encSelectTimes, frameEvents } =
  useRecording(recording);

// ── Render loop ───────────────────────────────────────────────────────────────

const { onBeforeRender } = useLoop();

onBeforeRender(({ elapsed, delta }) => {
  const pt = elapsed % duration;

  // Case transparency lerp
  const targetOpacity = props.hovered ? 0.35 : 1.0;
  for (const mat of caseMaterials) {
    mat.opacity += (targetOpacity - mat.opacity) * Math.min(1, delta * 6);
  }

  // Sliders
  for (const { knob, knobInfo, lever, leverInfo, index } of sliderTargets) {
    const fi = bsearch(sliderEvents[index], pt);
    const value = fi >= 0 ? sliderEvents[index][fi].value : 0;
    const dy = (value / 4095) * SLIDER_TRAVEL;
    if (knob  && knobInfo)  knob.position.copy(knobInfo.restLocal).addScaledVector(knobInfo.localDir, dy);
    if (lever && leverInfo) lever.position.copy(leverInfo.restLocal).addScaledVector(leverInfo.localDir, dy);
  }

  // Encoder rotation + select dip
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
        dip = dt < CLICK_HALF_DUR
          ? dt / CLICK_HALF_DUR
          : 1 - (dt - CLICK_HALF_DUR) / CLICK_HALF_DUR;
      }
    }
    encObj.position.copy(encRestLocal).addScaledVector(encPressLocalDir, dip * CLICK_DIP);
  }

  // Screen frame
  const canvas = screenCanvas;
  const tex = canvasTex;
  if (canvas && tex && gifBitmaps.length > 0 && frameEvents.length > 0) {
    const fi = bsearch(frameEvents, pt);
    if (fi >= 0) {
      const bitmap = gifBitmaps[frameEvents[fi].frame];
      if (bitmap) {
        canvas.getContext("2d")!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
        tex.needsUpdate = true;
      }
    }
  }
});
</script>

<template>
  <OrbitControls :enable-damping="false" :screen-space-panning="true" :max-distance="maxDistance" />
  <primitive :object="root" />
  <primitive v-if="screenPlaneMesh" :object="screenPlaneMesh" />
</template>
