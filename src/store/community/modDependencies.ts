// 前置 Mod 解析（镜像 Rust game::community::ModDependencyItem/Plan；命令 resolve_mod_dependencies）。
import { invoke } from "@tauri-apps/api/core";

export interface ModDependencyItem {
	project_id: string;
	version_id: string;
	name: string;
	version_label: string;
	loader: string;
	game_version: string;
	url: string;
	filename: string;
	relation: "required" | "optional" | string;
	root_optional: string | null;
	installed: boolean;
	error: string | null;
}

export interface ModDependencyPlan {
	instance_mc: string;
	instance_loader: string;
	items: ModDependencyItem[];
	errors: string[];
}

export function resolveModDependencies(payload: {
	source: string;
	projectId: string;
	versionId: string;
	versionDirName: string;
}): Promise<ModDependencyPlan> {
	return invoke<ModDependencyPlan>("resolve_mod_dependencies", payload);
}
