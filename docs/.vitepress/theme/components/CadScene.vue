<script setup lang="ts">
import { useTresContext, useLoop } from "@tresjs/core";
import { Bounds, OrbitControls } from "@tresjs/cientos";
import {
  AnimationMixer,
  Box3,
  EdgesGeometry,
  LineBasicMaterial,
  LineSegments,
  LoopRepeat,
  PMREMGenerator,
  Vector3,
  WebGLRenderer,
} from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import { GLTFAnimationPointerExtension } from "@needle-tools/three-animation-pointer";

const props = defineProps<{ url: string; animate?: boolean }>();

// inject calls must happen before any await
const { scene, renderer } = useTresContext();

const loader = new GLTFLoader();
loader.register((parser) => new GLTFAnimationPointerExtension(parser));
const gltf = await loader.loadAsync(props.url);
const gltfScene = gltf.scene;

// Center model at origin (mirrors three-gltf-viewer setContent)
gltfScene.updateMatrixWorld();
const box = new Box3().setFromObject(gltfScene);
const center = new Vector3();
const sizeVec = new Vector3();
box.getCenter(center);
box.getSize(sizeVec);
const modelSize = sizeVec.length();

gltfScene.position.x -= center.x;
gltfScene.position.y -= center.y;
gltfScene.position.z -= center.z;

// RoomEnvironment IBL (neutral, no background)
const pmrem = new PMREMGenerator(renderer.instance as WebGLRenderer);
scene.value.environment = pmrem.fromScene(new RoomEnvironment()).texture;
pmrem.dispose();

const maxDistance = modelSize * 10;

const edgeMaterial = new LineBasicMaterial({ color: 0x000000, opacity: 0.25, transparent: true });
gltfScene.traverse((node: any) => {
  if (!node.isMesh) return;

  // Add sharp edges (threshold 20° keeps curved surfaces clean)
  const edges = new EdgesGeometry(node.geometry, 20);
  const lines = new LineSegments(edges, edgeMaterial);
  lines.raycast = () => {}; // don't interfere with interaction
  node.add(lines);
});

// Animation playback
if (props.animate && gltf.animations.length > 0) {
  const mixer = new AnimationMixer(gltfScene);
  for (const clip of gltf.animations) {
    const action = mixer.clipAction(clip);
    action.setLoop(LoopRepeat, Infinity);
    action.play();
  }

  // Find ScreenPlane and its frame-timing extras (stored by animate_assembly.py).
  // We drive the texture offset manually since KHR_animation_pointer is unreliable.
  let screenEmissiveMap: any = null;
  let screenNCols = 1, screenNRows = 1;
  let screenFrames: Array<{ t: number; col: number; row: number }> = [];

  gltfScene.traverse((node: any) => {
    console.log("[CadScene] node:", node.name, "userData keys:", Object.keys(node.userData ?? {}));
    if (node.name === "ScreenPlane") {
      console.log("[CadScene] Found ScreenPlane, userData:", node.userData);
      const mat = Array.isArray(node.material) ? node.material[0] : node.material;
      console.log("[CadScene] ScreenPlane material:", mat?.name, "emissiveMap:", mat?.emissiveMap);
    }
    if (node.name !== "ScreenPlane" || !node.userData?.screen_animation) return;
    const data = JSON.parse(node.userData.screen_animation);
    screenNCols = data.n_cols;
    screenNRows = data.n_rows;
    screenFrames = data.frames; // [{t, col, row}, ...] sorted by t
    console.log("[CadScene] screen_animation parsed:", screenNCols, "×", screenNRows, "cols×rows,", screenFrames.length, "frames, first:", screenFrames[0], "last:", screenFrames.at(-1));
    const mat = Array.isArray(node.material) ? node.material[0] : node.material;
    if (mat?.emissiveMap) screenEmissiveMap = mat.emissiveMap;
    console.log("[CadScene] emissiveMap assigned:", screenEmissiveMap);
    console.log("[CadScene] initial offset:", screenEmissiveMap?.offset, "repeat:", screenEmissiveMap?.repeat);
  });

  const { onBeforeRender } = useLoop();
  let debugLogged = false;
  onBeforeRender(({ delta }) => {
    mixer.update(delta);

    if (!screenEmissiveMap || screenFrames.length === 0) {
      if (!debugLogged) {
        console.log("[CadScene] render loop: screenEmissiveMap=", screenEmissiveMap, "screenFrames.length=", screenFrames.length);
        debugLogged = true;
      }
      return;
    }

    // Step-function: find last frame with t <= mixer.time
    const t = mixer.time;
    let lo = 0, hi = screenFrames.length - 1, fi = 0;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (screenFrames[mid].t <= t) { fi = mid; lo = mid + 1; }
      else hi = mid - 1;
    }
    const { col, row } = screenFrames[fi];
    if (!debugLogged) {
      console.log("[CadScene] first texture update: t=", t, "fi=", fi, "col=", col, "row=", row);
      console.log("[CadScene] pre-set offset:", screenEmissiveMap.offset.clone(), "repeat:", screenEmissiveMap.repeat.clone());
      console.log("[CadScene] computed offsetX=", col / screenNCols, "offsetY=", (screenNRows - row - 1) / screenNRows);
      debugLogged = true;
    }
    // Y is flipped: pixel row 0 (top) maps to UV Y = (nRows-1)/nRows
    screenEmissiveMap.offset.set(col / screenNCols, (screenNRows - row - 1) / screenNRows);
    screenEmissiveMap.needsUpdate = true;
  });
}
</script>

<template>
  <OrbitControls :enable-damping="false" :screen-space-panning="true" :max-distance="maxDistance" make-default />
  <Bounds use-mounted clip>
    <primitive :object="gltfScene" />
  </Bounds>
</template>
