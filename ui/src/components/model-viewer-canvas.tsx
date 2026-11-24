/**
 * three.js viewer for STL, OBJ and 3MF. Loaded lazily (see model-viewer.tsx): it pulls in three.js.
 */
import * as React from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
import { ThreeMFLoader } from "three/examples/jsm/loaders/3MFLoader.js";
import { Box as BoxIcon, Loader2, RotateCcw } from "lucide-react";

import { cn } from "@/lib/utils";
import type { ViewerFormat } from "@/lib/viewer";

type Stats = { size: [number, number, number]; triangles: number };

/** Neutral body colour the accent is mixed into (see `--signal` in index.css). */
const NEUTRAL = new THREE.Color(0x9fb4c7);

type Tint = "neutral" | "mist" | "tint" | "glow";
type TintParams = { color: THREE.Color; emissive: THREE.Color; emissiveIntensity: number };

const TINTS: { id: Tint; label: string }[] = [
  { id: "neutral", label: "Neutral" },
  { id: "mist", label: "Mist" },
  { id: "tint", label: "Tint" },
  { id: "glow", label: "Glow" },
];

/** How much of the accent colour goes into the body, per tint. */
const TINT_MIX: Record<Tint, number> = { neutral: 0, mist: 0.18, tint: 0.35, glow: 0.18 };

/** The body colour for a tint: the neutral colour lerped toward the accent. */
function tintBody(signal: string, tint: Tint): THREE.Color {
  return NEUTRAL.clone().lerp(new THREE.Color(signal), TINT_MIX[tint]);
}

function tintParams(signal: string, tint: Tint): TintParams {
  const accent = new THREE.Color(signal);
  return {
    color: tintBody(signal, tint),
    emissive: tint === "glow" ? accent : new THREE.Color(0x000000),
    emissiveIntensity: tint === "glow" ? 0.08 : 0,
  };
}

/** The body material, tagged so the tint only recolours the parts we painted (not 3MF colours). */
function bodyMaterial(): THREE.MeshStandardMaterial {
  const material = new THREE.MeshStandardMaterial({ color: NEUTRAL, roughness: 0.6, metalness: 0.08, flatShading: false });
  material.userData.limaBody = true;
  return material;
}

const TINT_KEY = "lima.viewer.tint";

function readTint(): Tint {
  try {
    const saved = localStorage.getItem(TINT_KEY);
    if (saved === "neutral" || saved === "mist" || saved === "tint" || saved === "glow") return saved;
  } catch {
    // localStorage unavailable; fall through to the default.
  }
  return "mist";
}

function writeTint(tint: Tint) {
  try {
    localStorage.setItem(TINT_KEY, tint);
  } catch {
    // localStorage unavailable.
  }
}

/** The accent colour the current theme exposes (see `--signal`). */
function readSignal(): string {
  return getComputedStyle(document.documentElement).getPropertyValue("--signal").trim() || "#0e9f83";
}

async function loadObject(url: string, format: ViewerFormat, onProgress: (p: number) => void, material: () => THREE.MeshStandardMaterial): Promise<THREE.Object3D> {
  const progress = (e: ProgressEvent) => e.total && onProgress(e.loaded / e.total);
  if (format === "stl") {
    const geometry = await new STLLoader().loadAsync(url, progress);
    if (!geometry.attributes.normal) geometry.computeVertexNormals();
    return new THREE.Mesh(geometry, material());
  }
  if (format === "obj") {
    const group = await new OBJLoader().loadAsync(url, progress);
    group.traverse((o) => {
      if (o instanceof THREE.Mesh) o.material = material();
    });
    return group;
  }
  // 3MF keeps its own colours (multi-colour prints); untextured parts get ours.
  const group = await new ThreeMFLoader().loadAsync(url, progress);
  group.traverse((o) => {
    if (o instanceof THREE.Mesh && !o.material) o.material = material();
  });
  return group;
}

/** `LMP1` mesh from the server: header, positions, normals, indices (all little-endian). */
async function loadPreview(
  url: string,
  signal: AbortSignal,
  onProgress: (p: number) => void,
  material: () => THREE.MeshStandardMaterial,
): Promise<THREE.Object3D> {
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`Preview request failed (${response.status})`);
  const total = Number(response.headers.get("content-length")) || 0;
  const chunks: Uint8Array[] = [];
  let received = 0;
  if (response.body) {
    const reader = response.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      received += value.length;
      if (total > 0) onProgress(Math.min(received / total, 1));
    }
  } else {
    const whole = new Uint8Array(await response.arrayBuffer());
    chunks.push(whole);
    received = whole.length;
  }

  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  if (bytes.length < 12 || String.fromCharCode(...bytes.subarray(0, 4)) !== "LMP1") {
    throw new Error("Unrecognised preview mesh");
  }
  const view = new DataView(bytes.buffer);
  const vertexCount = view.getUint32(4, true);
  const indexCount = view.getUint32(8, true);
  const positionsAt = 12;
  const normalsAt = positionsAt + vertexCount * 12;
  const indicesAt = normalsAt + vertexCount * 12;
  if (indicesAt + indexCount * 4 > bytes.length) throw new Error("Truncated preview mesh");

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(bytes.buffer, positionsAt, vertexCount * 3), 3));
  geometry.setAttribute("normal", new THREE.BufferAttribute(new Float32Array(bytes.buffer, normalsAt, vertexCount * 3), 3));
  geometry.setIndex(new THREE.BufferAttribute(new Uint32Array(bytes.buffer, indicesAt, indexCount), 1));
  return new THREE.Mesh(geometry, material());
}

function countTriangles(obj: THREE.Object3D): number {
  let n = 0;
  obj.traverse((o) => {
    if (o instanceof THREE.Mesh) {
      const g = o.geometry as THREE.BufferGeometry;
      n += (g.index ? g.index.count : g.attributes.position.count) / 3;
    }
  });
  return Math.round(n);
}

function disposeObject(obj: THREE.Object3D) {
  obj.traverse((o) => {
    if (o instanceof THREE.Mesh) {
      o.geometry.dispose();
      for (const m of Array.isArray(o.material) ? o.material : [o.material]) m.dispose();
    }
  });
}

export default function ModelViewerCanvas({ url, format, previewUrl }: { url: string; format: ViewerFormat; previewUrl?: string }) {
  const mountRef = React.useRef<HTMLDivElement | null>(null);
  const api = React.useRef<{ reset: () => void; wireframe: (on: boolean) => void; tint: (params: TintParams) => void } | null>(null);
  const [progress, setProgress] = React.useState(0);
  const [loaded, setLoaded] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [stats, setStats] = React.useState<Stats | null>(null);
  const [wire, setWire] = React.useState(false);
  const [tint, setTint] = React.useState<Tint>(readTint);
  const [signal, setSignal] = React.useState(readSignal);
  const tintRef = React.useRef(tint);
  const signalRef = React.useRef(signal);

  React.useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    let disposed = false;
    let frame = 0;

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(mount.clientWidth, mount.clientHeight);
    mount.appendChild(renderer.domElement);
    renderer.domElement.className = "block h-full w-full touch-none";

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(35, mount.clientWidth / Math.max(mount.clientHeight, 1), 0.1, 100000);
    scene.add(camera);
    scene.add(new THREE.HemisphereLight(0xffffff, 0x3a4250, 1.6));
    // A light that follows the camera keeps the side you look at lit.
    const key = new THREE.DirectionalLight(0xffffff, 1.8);
    key.position.set(1, 1.5, 2);
    camera.add(key);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;

    const resize = new ResizeObserver(() => {
      const w = mount.clientWidth;
      const h = Math.max(mount.clientHeight, 1);
      renderer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    });
    resize.observe(mount);

    const loop = () => {
      frame = requestAnimationFrame(loop);
      controls.update();
      renderer.render(scene, camera);
    };
    loop();

    let object: THREE.Object3D | null = null;
    let grid: THREE.GridHelper | null = null;
    const controller = previewUrl ? new AbortController() : null;

    const loading = previewUrl
      ? loadPreview(previewUrl, controller!.signal, setProgress, bodyMaterial)
      : loadObject(url, format, setProgress, bodyMaterial);
    loading
      .then((obj) => {
        if (disposed) return disposeObject(obj);
        object = obj;

        // Model size before reorienting (printing files are in mm, Z up).
        const raw = new THREE.Box3().setFromObject(obj).getSize(new THREE.Vector3());
        setStats({ size: [raw.x, raw.y, raw.z], triangles: countTriangles(obj) });

        // STL and 3MF are Z-up; three.js is Y-up. Server previews are always Z-up.
        if (previewUrl || format !== "obj") obj.rotation.x = -Math.PI / 2;
        obj.updateMatrixWorld(true);
        const box = new THREE.Box3().setFromObject(obj);
        const center = box.getCenter(new THREE.Vector3());
        // Sit it on the grid, centred.
        obj.position.set(-center.x, -box.min.y, -center.z);
        scene.add(obj);

        const size = box.getSize(new THREE.Vector3());
        const span = Math.max(size.x, size.y, size.z) || 1;
        const gridSize = Math.ceil((Math.max(size.x, size.z) * 1.6) / 10) * 10 || 10;
        grid = new THREE.GridHelper(gridSize, Math.min(gridSize / 10, 60), 0x4b5563, 0x2a313b);
        scene.add(grid);

        const target = new THREE.Vector3(0, size.y / 2, 0);
        const distance = (span / 2 / Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.5;
        const home = new THREE.Vector3(1, 0.75, 1.2).normalize().multiplyScalar(distance).add(target);
        camera.near = distance / 1000;
        camera.far = distance * 100;
        camera.updateProjectionMatrix();
        const reset = () => {
          camera.position.copy(home);
          controls.target.copy(target);
          controls.update();
        };
        reset();
        api.current = {
          reset,
          wireframe: (on) =>
            obj.traverse((o) => {
              if (o instanceof THREE.Mesh) for (const m of Array.isArray(o.material) ? o.material : [o.material]) m.wireframe = on;
            }),
          tint: (params) =>
            obj.traverse((o) => {
              if (o instanceof THREE.Mesh) {
                for (const m of Array.isArray(o.material) ? o.material : [o.material]) {
                  if (m instanceof THREE.MeshStandardMaterial && m.userData.limaBody) {
                    m.color.copy(params.color);
                    m.emissive.copy(params.emissive);
                    m.emissiveIntensity = params.emissiveIntensity;
                  }
                }
              }
            }),
        };
        // Paint with the current tint/theme before showing (loading may have overlapped a change).
        api.current.tint(tintParams(signalRef.current, tintRef.current));
        setLoaded(true);
      })
      .catch((e: unknown) => {
        if (!disposed) setError(e instanceof Error ? e.message : String(e));
      });

    return () => {
      disposed = true;
      controller?.abort();
      cancelAnimationFrame(frame);
      resize.disconnect();
      controls.dispose();
      if (object) disposeObject(object);
      grid?.dispose();
      renderer.dispose();
      renderer.domElement.remove();
      api.current = null;
    };
  }, [url, format, previewUrl]);

  // Persist the choice and repaint the loaded model when it changes.
  React.useEffect(() => {
    tintRef.current = tint;
    writeTint(tint);
    api.current?.tint(tintParams(signalRef.current, tint));
  }, [tint]);

  // Repaint when the theme flips and `--signal` changes.
  React.useEffect(() => {
    signalRef.current = signal;
    api.current?.tint(tintParams(signal, tintRef.current));
  }, [signal]);

  // The theme toggles a class on <html>; re-read `--signal` and recolor live.
  React.useEffect(() => {
    const observer = new MutationObserver(() => setSignal(readSignal()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "data-theme"] });
    return () => observer.disconnect();
  }, []);

  return (
    <div className="absolute inset-0">
      <div ref={mountRef} className="absolute inset-0" />

      {!loaded && !error ? (
        <div className="pointer-events-none absolute inset-0 grid place-items-center">
          <div className="flex items-center gap-2 rounded-full bg-black/60 px-3 py-1.5 font-mono text-xs text-white/85">
            <Loader2 className="size-3.5 animate-spin" />
            {progress > 0 ? `loading model · ${Math.round(progress * 100)}%` : "loading model"}
          </div>
        </div>
      ) : null}

      {error ? (
        <div className="absolute inset-0 grid place-items-center p-6 text-center">
          <div className="max-w-sm space-y-1.5">
            <BoxIcon className="mx-auto size-6 text-dim" />
            <div className="text-sm font-medium">Can’t preview this file</div>
            <p className="text-xs text-muted-foreground">
              The 3D loader couldn’t read it (some slicer 3MF features aren’t supported). Download it or open it in a slicer instead.
            </p>
            <p className="font-mono text-[11px] break-words text-dim">{error}</p>
          </div>
        </div>
      ) : null}

      {loaded ? (
        <>
          <div className="absolute right-4 bottom-4 z-[3] flex items-center gap-1.5">
            <div className="flex items-center gap-1 rounded-full border border-white/15 bg-black/55 p-1 backdrop-blur">
              {TINTS.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => setTint(t.id)}
                  aria-label={`Model colour: ${t.label}`}
                  title={`Model colour: ${t.label}`}
                  className={cn(
                    "size-3.5 rounded-full border border-white/20",
                    tint === t.id && "ring-2 ring-signal ring-offset-1 ring-offset-black/40",
                  )}
                  style={{ background: `#${tintBody(signal, t.id).getHexString()}` }}
                />
              ))}
            </div>
            <button
              type="button"
              onClick={() => {
                setWire(!wire);
                api.current?.wireframe(!wire);
              }}
              aria-pressed={wire}
              className={cn(
                "rounded-full border border-white/15 bg-black/55 px-3 py-1.5 font-mono text-[11px] text-white backdrop-blur hover:bg-black/75",
                wire && "border-signal text-signal",
              )}
            >
              wireframe
            </button>
            <button
              type="button"
              onClick={() => api.current?.reset()}
              aria-label="Reset view"
              title="Reset view"
              className="grid size-8 place-items-center rounded-full border border-white/15 bg-black/55 text-white backdrop-blur hover:bg-black/75"
            >
              <RotateCcw className="size-3.5" />
            </button>
          </div>
          {stats ? (
            <div className="absolute bottom-4 left-4 z-[3] rounded bg-black/60 px-2 py-0.5 font-mono text-[11px] text-white/80">
              {stats.size.map((v) => v.toFixed(1)).join(" × ")} mm · {stats.triangles.toLocaleString()} tris
              {previewUrl ? " · preview" : ""}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
