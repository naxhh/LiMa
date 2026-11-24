import type { ViewerFormat } from "@/lib/viewer";

/** One mesh sent from the loader worker. Typed arrays are transferred, not copied. */
export type WireMesh = {
  positions: Float32Array;
  normals: Float32Array | null;
  colors: Float32Array | null;
  index: Uint32Array | null;
  /** Local-to-world matrix, 16 values in column-major order. */
  matrix: number[];
  materials: WireMaterial[];
  groups: { start: number; count: number; materialIndex: number }[];
};

export type WireMaterial = {
  color: number;
  opacity: number;
  /** The viewer's neutral body material, so the tint controls recolour it. */
  body: boolean;
  vertexColors: boolean;
};

export type WorkerRequest = { url: string; format: ViewerFormat };

export type WorkerResponse =
  | { type: "progress"; loaded: number; total: number }
  | { type: "done"; meshes: WireMesh[] }
  | { type: "error"; message: string };
