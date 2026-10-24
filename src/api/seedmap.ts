// 种子预览 IPC 封装：种子与版本原样传后端，版本/字符串种子解析在 Rust 侧完成。
import { invoke } from "@tauri-apps/api/core";
import type { ResolvedVersion } from "../types/seedmap/ResolvedVersion";
import type { SeedMapBlock } from "../types/seedmap/SeedMapBlock";
import type { SeedMapSlimeChunk } from "../types/seedmap/SeedMapSlimeChunk";
import type { SeedMapStructure } from "../types/seedmap/SeedMapStructure";
import type { SeedMapVersion } from "../types/seedmap/SeedMapVersion";

export type {
  ResolvedVersion,
  SeedMapBlock,
  SeedMapSlimeChunk,
  SeedMapStructure,
  SeedMapVersion,
};

function bytesOf(payload: unknown): Uint8Array {
  if (payload instanceof ArrayBuffer) return new Uint8Array(payload);
  if (payload instanceof Uint8Array) return payload;
  if (Array.isArray(payload)) return Uint8Array.from(payload as number[]);
  return new Uint8Array(payload as ArrayBufferLike);
}

export function getSeedMapVersions(): Promise<SeedMapVersion[]> {
  return invoke<SeedMapVersion[]>("get_seed_map_versions");
}

export function resolveSeedMapVersion(version: string): Promise<ResolvedVersion> {
  return invoke<ResolvedVersion>("resolve_seed_map_version", { version });
}

export async function getSeedMapTile(
  seed: string,
  version: string,
  rx: number,
  rz: number,
): Promise<Blob> {
  const payload = await invoke<ArrayBuffer>("get_seed_map_tile", { seed, version, rx, rz });
  const bytes = bytesOf(payload);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy.buffer], { type: "image/png" });
}

export function getSeedMapBlock(
  seed: string,
  version: string,
  x: number,
  z: number,
): Promise<SeedMapBlock> {
  return invoke<SeedMapBlock>("get_seed_map_block", { seed, version, x, z });
}

export function getSeedMapStructures(
  seed: string,
  version: string,
  rx: number,
  rz: number,
): Promise<SeedMapStructure[]> {
  return invoke<SeedMapStructure[]>("get_seed_map_structures", {
    seed,
    version,
    rx,
    rz,
  });
}

export function findSeedMapStructures(
  seed: string,
  version: string,
  kind: string,
  centerX: number,
  centerZ: number,
  radius: number,
): Promise<SeedMapStructure[]> {
  return invoke<SeedMapStructure[]>("find_seed_map_structures", {
    seed,
    version,
    kind,
    centerX,
    centerZ,
    radius,
  });
}

export function findSeedMapSlimeChunks(
  seed: string,
  version: string,
  centerX: number,
  centerZ: number,
  radius: number,
): Promise<SeedMapSlimeChunk[]> {
  return invoke<SeedMapSlimeChunk[]>("find_seed_map_slime_chunks", {
    seed,
    version,
    centerX,
    centerZ,
    radius,
  });
}

export function getSeedMapSlimeChunks(
  seed: string,
  version: string,
  rx: number,
  rz: number,
): Promise<SeedMapSlimeChunk[]> {
  return invoke<SeedMapSlimeChunk[]>("get_seed_map_slime_chunks", {
    seed,
    version,
    rx,
    rz,
  });
}
