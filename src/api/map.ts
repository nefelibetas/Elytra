// 世界预览 IPC 封装：版本 + 存档名由后端解析为世界根目录，前端不传任意路径。

import { invoke } from "@tauri-apps/api/core";
import type { MapBlockInfo } from "../types/map/MapBlockInfo";
import type { MapRegionInfo } from "../types/map/MapRegionInfo";
import type { MapWorldInfo } from "../types/map/MapWorldInfo";

export type { MapBlockInfo, MapRegionInfo, MapWorldInfo };

export function getMapWorldInfo(versionName: string, worldName: string): Promise<MapWorldInfo> {
  return invoke<MapWorldInfo>("get_map_world_info", { versionName, worldName });
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
  style: string,
  modColors: boolean,
): Promise<Blob> {
  const payload = await invoke<ArrayBuffer>("get_map_tile", {
    versionName,
    worldName,
    dimension,
    rx,
    rz,
    style,
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
  y?: number,
): Promise<MapBlockInfo> {
  return invoke<MapBlockInfo>("get_map_block", {
    versionName,
    worldName,
    dimension,
    x,
    z,
    y,
  });
}
