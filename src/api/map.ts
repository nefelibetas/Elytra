// 世界预览 IPC 封装：版本 + 存档名由后端解析为世界根目录，前端不传任意路径。

import { invoke } from "@tauri-apps/api/core";
import type { MapBlockInfo } from "../types/map/MapBlockInfo";
import type { MapBiomeHit } from "../types/map/MapBiomeHit";
import type { MapBiomeSummary } from "../types/map/MapBiomeSummary";
import type { MapFolderEntry } from "../types/map/MapFolderEntry";
import type { MapRegionInfo } from "../types/map/MapRegionInfo";
import type { MapWorldInfo } from "../types/map/MapWorldInfo";
import type { SeedMapSlimeChunk } from "../types/seedmap/SeedMapSlimeChunk";

export type {
  MapBiomeHit,
  MapBiomeSummary,
  MapBlockInfo,
  MapFolderEntry,
  MapRegionInfo,
  MapWorldInfo,
  SeedMapSlimeChunk,
};

export function getMapWorldInfo(versionName: string, worldName: string): Promise<MapWorldInfo> {
  return invoke<MapWorldInfo>("get_map_world_info", { versionName, worldName });
}

export function resolveMapFolders(path: string): Promise<MapFolderEntry[]> {
  return invoke<MapFolderEntry[]>("resolve_map_folders", { path });
}

export function getMapFolderInfo(root: string): Promise<MapWorldInfo> {
  return invoke<MapWorldInfo>("get_map_folder_info", { root });
}

export function listMapFolderRegions(
  root: string,
  dimension: string,
): Promise<MapRegionInfo[]> {
  return invoke<MapRegionInfo[]>("list_map_folder_regions", { root, dimension });
}

export function listMapRegions(
  versionName: string,
  worldName: string,
  dimension: string,
): Promise<MapRegionInfo[]> {
  return invoke<MapRegionInfo[]>("list_map_regions", {
    versionName,
    worldName,
    dimension,
  });
}

/** Tauri ipc::Response 大载荷走 ArrayBuffer，小载荷可能退化为 JSON 数字数组。 */
function bytesOf(payload: unknown): Uint8Array {
  if (payload instanceof ArrayBuffer) return new Uint8Array(payload);
  if (payload instanceof Uint8Array) return payload;
  if (Array.isArray(payload)) return Uint8Array.from(payload as number[]);
  return new Uint8Array(payload as ArrayBufferLike);
}

export async function getMapTile(
  versionName: string,
  worldName: string,
  dimension: string,
  rx: number,
  rz: number,
  maxY: number,
  modColors: boolean,
): Promise<Blob> {
  const payload = await invoke<ArrayBuffer>("get_map_tile", {
    versionName,
    worldName,
    dimension,
    rx,
    rz,
    maxY,
    modColors,
  });
  const bytes = bytesOf(payload);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy.buffer], { type: "image/png" });
}

export function getMapBlock(
  versionName: string,
  worldName: string,
  dimension: string,
  x: number,
  z: number,
  maxY: number,
): Promise<MapBlockInfo> {
  return invoke<MapBlockInfo>("get_map_block", {
    versionName,
    worldName,
    dimension,
    x,
    z,
    maxY,
  });
}

export async function getMapFolderTile(
  root: string,
  dimension: string,
  rx: number,
  rz: number,
  maxY: number,
  modColors: boolean,
): Promise<Blob> {
  const payload = await invoke<ArrayBuffer>("get_map_folder_tile", {
    root,
    dimension,
    rx,
    rz,
    maxY,
    modColors,
  });
  const bytes = bytesOf(payload);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy.buffer], { type: "image/png" });
}

export function getMapFolderBlock(
  root: string,
  dimension: string,
  x: number,
  z: number,
  maxY: number,
): Promise<MapBlockInfo> {
  return invoke<MapBlockInfo>("get_map_folder_block", {
    root,
    dimension,
    x,
    z,
    maxY,
  });
}

/** 世界预览史莱姆覆盖层：单个 region 的区块坐标。 */
export function getMapSlimeChunks(
  seed: string,
  rx: number,
  rz: number,
): Promise<SeedMapSlimeChunk[]> {
  return invoke<SeedMapSlimeChunk[]>("get_map_slime_chunks", { seed, rx, rz });
}

/** 世界预览史莱姆搜索：中心附近最近的 32 个史莱姆区块。 */
export function findMapSlimeChunks(
  seed: string,
  centerX: number,
  centerZ: number,
  radius: number,
): Promise<SeedMapSlimeChunk[]> {
  return invoke<SeedMapSlimeChunk[]>("find_map_slime_chunks", {
    seed,
    centerX,
    centerZ,
    radius,
  });
}

export function listMapBiomes(
  versionName: string,
  worldName: string,
  dimension: string,
  maxY: number,
): Promise<MapBiomeSummary[]> {
  return invoke<MapBiomeSummary[]>("list_map_biomes", {
    versionName,
    worldName,
    dimension,
    maxY,
  });
}

export function listMapFolderBiomes(
  root: string,
  dimension: string,
  maxY: number,
): Promise<MapBiomeSummary[]> {
  return invoke<MapBiomeSummary[]>("list_map_folder_biomes", { root, dimension, maxY });
}

export function findMapBiome(
  versionName: string,
  worldName: string,
  dimension: string,
  maxY: number,
  biome: string,
  centerX: number,
  centerZ: number,
): Promise<MapBiomeHit | null> {
  return invoke<MapBiomeHit | null>("find_map_biome", {
    versionName,
    worldName,
    dimension,
    maxY,
    biome,
    centerX,
    centerZ,
  });
}

export function findMapFolderBiome(
  root: string,
  dimension: string,
  maxY: number,
  biome: string,
  centerX: number,
  centerZ: number,
): Promise<MapBiomeHit | null> {
  return invoke<MapBiomeHit | null>("find_map_folder_biome", {
    root,
    dimension,
    maxY,
    biome,
    centerX,
    centerZ,
  });
}
