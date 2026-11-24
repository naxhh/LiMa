import * as THREE from "three";

import type { ViewerFormat } from "@/lib/viewer";
import type { WireMaterial, WireMesh, WorkerRequest, WorkerResponse } from "./model-geometry";

function material(data: WireMaterial): THREE.MeshStandardMaterial {
  const material = new THREE.MeshStandardMaterial({
    color: data.color,
    roughness: 0.6,
    metalness: 0.08,
    transparent: data.opacity < 1,
    opacity: data.opacity,
    vertexColors: data.vertexColors,
  });
  if (data.body) material.userData.limaBody = true;
  return material;
}

function object(meshes: WireMesh[]): THREE.Object3D {
  const root = new THREE.Group();
  for (const data of meshes) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
    if (data.normals) geometry.setAttribute("normal", new THREE.BufferAttribute(data.normals, 3));
    else geometry.computeVertexNormals();
    if (data.colors) geometry.setAttribute("color", new THREE.BufferAttribute(data.colors, 3));
    if (data.index) geometry.setIndex(new THREE.BufferAttribute(data.index, 1));

    const materials = data.materials.length > 0 ? data.materials.map(material) : [material({ color: 0x9fb4c7, opacity: 1, body: true, vertexColors: false })];
    for (const group of data.groups) {
      geometry.addGroup(group.start, group.count, Math.min(group.materialIndex, materials.length - 1));
    }
    const mesh = new THREE.Mesh(geometry, materials.length === 1 ? materials[0] : materials);
    mesh.matrixAutoUpdate = false;
    mesh.matrix.fromArray(data.matrix);
    root.add(mesh);
  }
  return root;
}

/**
 * Parses a model in a module worker and rebuilds it on the main thread from transferred arrays.
 * `cancel` terminates the worker so switching files doesn't leave a parse running.
 */
export function loadModelOffThread(
  url: string,
  format: ViewerFormat,
  onProgress: (progress: number) => void,
): { promise: Promise<THREE.Object3D>; cancel: () => void } {
  const worker = new Worker(new URL("./model-loader.worker.ts", import.meta.url), { type: "module" });
  let settled = false;
  let rejectPromise: ((error: Error) => void) | null = null;

  const cleanup = () => {
    worker.onmessage = null;
    worker.onerror = null;
    worker.terminate();
  };

  const promise = new Promise<THREE.Object3D>((resolve, reject) => {
    rejectPromise = reject;
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const message = event.data;
      if (message.type === "progress") {
        if (message.total > 0) onProgress(Math.min(message.loaded / message.total, 1));
        return;
      }
      settled = true;
      cleanup();
      if (message.type === "error") reject(new Error(message.message));
      else resolve(object(message.meshes));
    };
    worker.onerror = (event) => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new Error(event.message || "The model worker failed to start"));
    };
    worker.postMessage({ url, format } satisfies WorkerRequest);
  });

  const cancel = () => {
    if (settled) return;
    settled = true;
    cleanup();
    rejectPromise?.(new DOMException("Model loading was cancelled", "AbortError"));
  };

  return { promise, cancel };
}
