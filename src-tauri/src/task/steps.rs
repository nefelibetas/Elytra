// ── 步骤分析：任务 → 步骤序列（TaskSpec 统一入口，命令层与 runner 共用）──

use super::model::{DownloadFilesTask, DownloadTask, StepKind};

#[derive(Debug, Clone)]
pub struct StepSpec {
    pub name: String,
    pub kind: StepKind,
}

/// 任务 → 描述（任务名 + 步骤）统一入口：新任务类型实现本 trait 即获得步骤能力。
pub trait TaskSpec {
    fn name(&self) -> String;
    fn steps(&self) -> Vec<StepSpec>;
}

impl TaskSpec for DownloadTask {
    fn name(&self) -> String {
        match &self.loader {
            Some(l) => format!(
                "{} {}-{} 安装",
                self.mc_version,
                l.kind.display_name(),
                l.version
            ),
            None => format!("{} 下载", self.version_id),
        }
    }

    /// 步骤序列（顺序即执行顺序；零缺失维度由 runner 检测后过滤）。
    fn steps(&self) -> Vec<StepSpec> {
        let mut v = vec![
            StepSpec {
                name: "获取版本清单".into(),
                kind: StepKind::VersionJson,
            },
            StepSpec {
                name: "下载原版客户端文件".into(),
                kind: StepKind::ClientJar,
            },
            StepSpec {
                name: "下载原版支持库文件".into(),
                kind: StepKind::Libraries,
            },
            StepSpec {
                name: "下载原版资源文件".into(),
                kind: StepKind::Assets,
            },
            StepSpec {
                name: "处理 native 库".into(),
                kind: StepKind::Natives,
            },
        ];
        if let Some(l) = &self.loader {
            v.push(StepSpec {
                name: format!("下载 {} 本体", l.kind.display_name()),
                kind: StepKind::LoaderLibs,
            });
        }
        if self.optifine_patch.is_some() {
            v.push(StepSpec {
                name: "安装 OptiFine".into(),
                kind: StepKind::OptiFine,
            });
        }
        if self.fabric_api.is_some() {
            v.push(StepSpec {
                name: "下载 Fabric API".into(),
                kind: StepKind::FabricApi,
            });
        }
        // 生成最终配置放最后：依赖本体/库就绪，UI 收尾展示
        if let Some(l) = &self.loader {
            v.push(StepSpec {
                name: format!("生成 {} 最终配置", l.kind.display_name()),
                kind: StepKind::LoaderConfig,
            });
        }
        v
    }
}

impl TaskSpec for DownloadFilesTask {
    fn name(&self) -> String {
        format!("下载 {} 个文件", self.files.len())
    }

    /// 每文件一步（文件名作步骤名）。
    fn steps(&self) -> Vec<StepSpec> {
        self.files
            .iter()
            .map(|f| StepSpec {
                name: f.filename.clone(),
                kind: StepKind::File,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::model::test_valid_task as valid_task;

    #[test]
    fn download_task_steps_analysis() {
        // 仅原版：json/client/libs/assets/natives
        let names = |t: &DownloadTask| t.steps().iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&valid_task()),
            vec![
                "获取版本清单",
                "下载原版客户端文件",
                "下载原版支持库文件",
                "下载原版资源文件",
                "处理 native 库"
            ]
        );

        // Fabric + Fabric API：追加 下载本体/Fabric API/生成配置(最后)
        let mut t = valid_task();
        t.loader = Some(crate::task::model::LoaderChoice {
            kind: crate::task::model::LoaderKind::Fabric,
            version: "0.19.3".into(),
        });
        t.fabric_api = Some(crate::task::model::FabricApi {
            url: "u".into(),
            version: "15.0.0".into(),
        });
        let n = names(&t);
        assert_eq!(
            n,
            vec![
                "获取版本清单",
                "下载原版客户端文件",
                "下载原版支持库文件",
                "下载原版资源文件",
                "处理 native 库",
                "下载 Fabric 本体",
                "下载 Fabric API",
                "生成 Fabric 最终配置"
            ]
        );

        // OptiFine：追加步骤
        let mut t = valid_task();
        t.optifine_patch = Some("G8".into());
        assert_eq!(names(&t).last().unwrap(), "安装 OptiFine");

        // 任务名
        assert_eq!(valid_task().name(), "26.2 下载");
        let mut t = valid_task();
        t.loader = Some(crate::task::model::LoaderChoice {
            kind: crate::task::model::LoaderKind::Quilt,
            version: "1.0".into(),
        });
        assert_eq!(t.name(), "26.2 Quilt-1.0 安装");
    }
}
