/**
 * Parses STL, OBJ and 3MF off the main thread.
 *
 * Large slicer 3MFs are 100+ MB of XML; three.js parses them synchronously and freezes the
 * page. Loaders run here instead, and only compact typed arrays cross back to the UI.
 */
import * as THREE from "three";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
import { ThreeMFLoader } from "three/examples/jsm/loaders/3MFLoader.js";
import { DOMParser as XmlDOMParser } from "@xmldom/xmldom";

import type { WireMaterial, WireMesh, WorkerRequest, WorkerResponse } from "./model-geometry";
import { parseLarge3mf } from "./three-mf";

const NEUTRAL = 0x9fb4c7;
/** Above this, the DOM-based loader costs too much memory; use the streaming geometry reader. */
const LARGE_3MF_BYTES = 5 * 1024 * 1024;

const worker = self as unknown as {
  onmessage: ((event: MessageEvent<WorkerRequest>) => void) | null;
  postMessage: (message: WorkerResponse, transfer?: Transferable[]) => void;
};

// --- 3MF needs a DOM, which workers don't have -------------------------------------------------
// xmldom has no querySelector, but three.js's loader only asks for simple tag names.
function installDom() {
  const globals = globalThis as unknown as { DOMParser?: unknown; document?: unknown };
  globals.DOMParser = XmlDOMParser;

  const sample = new XmlDOMParser().parseFromString("<x/>", "text/xml");
  const selectors = (root: { getElementsByTagName: (tag: string) => ArrayLike<unknown> }, selector: string) => {
    const tag = selector.trim().split(/\s+/).pop() ?? "*";
    return Array.from(root.getElementsByTagName(tag));
  };
  const patch = (proto: object) => {
    const p = proto as {
      querySelector?: (selector: string) => unknown;
      querySelectorAll?: (selector: string) => unknown[];
    };
    p.querySelector ??= function (selector: string) {
      return selectors(this as { getElementsByTagName: (tag: string) => ArrayLike<unknown> }, selector)[0] ?? null;
    };
    p.querySelectorAll ??= function (selector: string) {
      return selectors(this as { getElementsByTagName: (tag: string) => ArrayLike<unknown> }, selector);
    };
  };
  patch(Object.getPrototypeOf(sample));
  patch(Object.getPrototypeOf(sample.documentElement));

  // Textured 3MFs create <img> elements. There is no image loading here; a quiet stub keeps
  // the geometry (the only thing the viewer uses) while dropping the textures.
  globals.document ??= {
    createElementNS: () => ({
      style: {},
      addEventListener() {},
      removeEventListener() {},
      setAttribute() {},
    }),
  };
}

installDom();

function bodyMaterial(): THREE.MeshStandardMaterial {
  const material = new THREE.MeshStandardMaterial({ color: NEUTRAL, roughness: 0.6, metalness: 0.08 });
  material.userData.limaBody = true;
  return material;
}

function parse(buffer: ArrayBuffer, format: WorkerRequest["format"]): THREE.Object3D {
  if (format === "stl") {
    const geometry = new STLLoader().parse(buffer);
    if (!geometry.attributes.normal) geometry.computeVertexNormals();
    return new THREE.Mesh(geometry, bodyMaterial());
  }
  if (format === "obj") {
    const group = new OBJLoader().parse(new TextDecoder().decode(buffer));
    group.traverse((o) => {
      if (o instanceof THREE.Mesh) o.material = bodyMaterial();
    });
    return group;
  }
  if (buffer.byteLength > LARGE_3MF_BYTES) {
    return new THREE.Mesh(parseLarge3mf(buffer), bodyMaterial());
  }
  return new ThreeMFLoader().parse(buffer);
}

function floatAttribute(attribute: THREE.BufferAttribute | THREE.InterleavedBufferAttribute | undefined): Float32Array | null {
  if (!attribute) return null;
  if (attribute instanceof THREE.InterleavedBufferAttribute) {
    const out = new Float32Array(attribute.count * attribute.itemSize);
    for (let i = 0; i < attribute.count; i++) {
      for (let c = 0; c < attribute.itemSize; c++) {
        out[i * attribute.itemSize + c] = attribute.getComponent(i, c);
      }
    }
    return out;
  }
  return new Float32Array(attribute.array as ArrayLike<number>);
}

function materialData(material: THREE.Material | undefined): WireMaterial {
  if (!material) return { color: NEUTRAL, opacity: 1, body: false, vertexColors: false };
  const coloured = material as THREE.Material & { color?: THREE.Color; vertexColors?: boolean };
  return {
    color: coloured.color instanceof THREE.Color ? coloured.color.getHex() : NEUTRAL,
    opacity: material.transparent ? material.opacity : 1,
    body: material.userData.limaBody === true,
    vertexColors: coloured.vertexColors === true,
  };
}

function serialize(root: THREE.Object3D): WireMesh[] {
  root.updateMatrixWorld(true);
  const meshes: WireMesh[] = [];
  root.traverse((object) => {
    if (!(object instanceof THREE.Mesh)) return;
    const geometry = object.geometry;
    const positions = floatAttribute(geometry.attributes.position);
    if (!positions) return;
    if (!geometry.attributes.normal) geometry.computeVertexNormals();
    const materials = (Array.isArray(object.material) ? object.material : [object.material]).map(materialData);
    meshes.push({
      positions,
      normals: floatAttribute(geometry.attributes.normal),
      colors: floatAttribute(geometry.attributes.color),
      index: geometry.index ? new Uint32Array(geometry.index.array) : null,
      matrix: object.matrixWorld.toArray(),
      materials,
      groups: geometry.groups.map((g: { start: number; count: number; materialIndex: number }) => ({ start: g.start, count: g.count, materialIndex: g.materialIndex })),
    });
  });
  return meshes;
}

async function fetchBytes(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Could not download the model (${response.status})`);
  const total = Number(response.headers.get("content-length")) || 0;
  if (!response.body) return response.arrayBuffer();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    loaded += value.length;
    worker.postMessage({ type: "progress", loaded, total });
  }
  const bytes = new Uint8Array(loaded);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes.buffer;
}

async function run({ url, format }: WorkerRequest) {
  try {
    const buffer = await fetchBytes(url);
    const meshes = serialize(parse(buffer, format));
    const transfer = new Set<ArrayBuffer>();
    for (const mesh of meshes) {
      transfer.add(mesh.positions.buffer as ArrayBuffer);
      if (mesh.normals) transfer.add(mesh.normals.buffer as ArrayBuffer);
      if (mesh.colors) transfer.add(mesh.colors.buffer as ArrayBuffer);
      if (mesh.index) transfer.add(mesh.index.buffer as ArrayBuffer);
    }
    worker.postMessage({ type: "done", meshes }, Array.from(transfer));
  } catch (error) {
    worker.postMessage({ type: "error", message: error instanceof Error ? error.message : String(error) });
  }
}

worker.onmessage = (event) => {
  void run(event.data);
};
