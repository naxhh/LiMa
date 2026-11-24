/**
 * Geometry-only 3MF reader for very large files.
 *
 * Three's loader needs a DOM and holds the whole XML tree in memory; a 13 MB lithophane can
 * expand to 117 MB of XML and use gigabytes. This streams the XML with saxes instead, keeping
 * only the vertex and triangle numbers, and ignores build transforms and materials (merged
 * preview, tinted like STL/OBJ). Small 3MFs keep using the full loader and its colours.
 */
import * as THREE from "three";
import { strFromU8, unzipSync } from "fflate";
import { SaxesParser } from "saxes";

function localName(name: string): string {
  return name.slice(name.lastIndexOf(":") + 1);
}

function readPart(xml: string, positions: number[], indices: number[]): void {
  const parser = new SaxesParser({ xmlns: false });
  let base = 0;
  let inMesh = false;

  parser.on("opentag", (tag) => {
    const name = localName(tag.name);
    if (name === "mesh") {
      inMesh = true;
      base = positions.length / 3;
      return;
    }
    if (!inMesh) return;
    if (name === "vertex") {
      const x = Number(tag.attributes.x);
      const y = Number(tag.attributes.y);
      const z = Number(tag.attributes.z);
      if (Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(z)) positions.push(x, y, z);
    } else if (name === "triangle") {
      const a = Number(tag.attributes.v1);
      const b = Number(tag.attributes.v2);
      const c = Number(tag.attributes.v3);
      if (Number.isInteger(a) && Number.isInteger(b) && Number.isInteger(c)) indices.push(base + a, base + b, base + c);
    }
  });
  parser.on("closetag", (tag) => {
    if (localName(tag.name) === "mesh") inMesh = false;
  });
  parser.write(xml).close();
}

/** Merges every mesh in every `.model` part, the same approximation the server thumbnails use. */
export function parseLarge3mf(buffer: ArrayBuffer): THREE.BufferGeometry {
  const files = unzipSync(new Uint8Array(buffer));
  const positions: number[] = [];
  const indices: number[] = [];
  for (const [name, bytes] of Object.entries(files)) {
    if (!name.toLowerCase().endsWith(".model")) continue;
    readPart(strFromU8(bytes), positions, indices);
  }
  if (indices.length === 0) throw new Error("This 3MF has no mesh geometry");

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(new THREE.BufferAttribute(new Uint32Array(indices), 1));
  geometry.computeVertexNormals();
  return geometry;
}
