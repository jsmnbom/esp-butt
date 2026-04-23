<script setup lang="ts">
import { useTresContext } from "@tresjs/core";
import { useGLTF, OrbitControls } from "@tresjs/cientos";
import {
  AmbientLight,
  Box3,
  DirectionalLight,
  EdgesGeometry,
  LineBasicMaterial,
  LineSegments,
  PMREMGenerator,
  Vector3,
  WebGLRenderer,
} from "three";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

const props = defineProps<{ url: string }>();

// inject calls must happen before any await
const { camera, scene, renderer } = useTresContext();

const { execute } = useGLTF(props.url);
const gltf = await execute();
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

// Dynamic near/far + camera position based on model size
const activeCam = camera.activeCamera.value;
if (activeCam && "near" in activeCam) {
  activeCam.near = modelSize / 100;
  activeCam.far = modelSize * 100;
  (activeCam as any).updateProjectionMatrix?.();
  activeCam.position.set(0, modelSize * 0.75, modelSize);
  activeCam.lookAt(new Vector3());

  // Attach lights to camera so they always illuminate from the viewer direction
  const ambient = new AmbientLight("#ffffff", 0.3);
  const dirLight = new DirectionalLight("#ffffff", 0.8 * Math.PI);
  dirLight.position.set(0.5, 0, 0.866); // ~60° elevation
  activeCam.add(ambient);
  activeCam.add(dirLight);
}

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
</script>

<template>
  <OrbitControls :enable-damping="false" :screen-space-panning="true" :max-distance="maxDistance" />
  <primitive :object="gltfScene" />
</template>
