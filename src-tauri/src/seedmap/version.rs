use super::ffi;
use super::model::{ResolvedVersion, SeedMapVersion};

const EXACT: &[(&str, &str)] = &[
    ("Beta 1.7", "Beta 1.7"),
    ("Beta 1.8", "Beta 1.8"),
    ("1.0", "1.0"),
    ("1.1", "1.1"),
    ("1.2", "1.2"),
    ("1.3", "1.3"),
    ("1.4", "1.4"),
    ("1.5", "1.5"),
    ("1.6", "1.6"),
    ("1.7.10", "1.7.10"),
    ("1.8.9", "1.8.9"),
    ("1.9.4", "1.9.4"),
    ("1.10.2", "1.10.2"),
    ("1.11.2", "1.11.2"),
    ("1.12.2", "1.12.2"),
    ("1.13.2", "1.13.2"),
    ("1.14.4", "1.14.4"),
    ("1.15.2", "1.15.2"),
    ("1.16.1", "1.16.1"),
    ("1.16.5", "1.16.5"),
    ("1.17.1", "1.17.1"),
    ("1.18.2", "1.18.2"),
    ("1.19.2", "1.19.2"),
    ("1.19.4", "1.19.4"),
    ("1.20.6", "1.20.6"),
    ("1.21.1", "1.21.1"),
    ("1.21.3", "1.21.3"),
    ("1.21.4", "1.21.4"),
    ("1.21.5", "1.21.5"),
    ("1.21.6", "1.21.6"),
    ("1.21.9", "1.21.9"),
    ("1.21.11", "1.21.11"),
    ("26.1", "26.1"),
    ("26.2", "26.2"),
    ("26.3", "26.3"),
];

pub fn supported_versions() -> Vec<SeedMapVersion> {
    EXACT
        .iter()
        .map(|(id, _)| SeedMapVersion {
            id: (*id).to_string(),
            label: (*id).to_string(),
        })
        .collect()
}

pub fn resolve_version(input: &str) -> Result<ResolvedVersion, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("版本不能为空\n请输入正式版 ID，如 26.3 或 1.20.1".to_string());
    }
    if is_snapshot(raw) {
        return Err("暂不支持快照\n请选择正式版；快照/预览版参数族尚无稳定数据".to_string());
    }
    if let Some((id, family)) = EXACT.iter().find(|(id, _)| *id == raw) {
        return build(raw, family, true).map(|_| ResolvedVersion {
            version: (*id).to_string(),
            family: (*family).to_string(),
            exact: true,
        });
    }
    let Some(family) = map_patch(raw) else {
        return Err(format!("版本不受支持\n{raw} 不在已校准的正式版参数族内"));
    };
    build(raw, &family, false)?;
    Ok(ResolvedVersion {
        version: raw.to_string(),
        family,
        exact: false,
    })
}

fn build(version: &str, family: &str, _exact: bool) -> Result<(), String> {
    if ffi::mc_from_name(family).is_none() {
        return Err(format!(
            "版本不受支持\n{version} 映射到的参数族 {family} 未编译进 cubiomes"
        ));
    }
    Ok(())
}

fn is_snapshot(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("snapshot")
        || lower.contains("-pre")
        || lower.contains("-rc")
        || lower.contains("pre-release")
        || lower.contains("release-candidate")
        || lower.contains("experimental")
    {
        return true;
    }
    let b = raw.as_bytes();
    b.len() >= 3 && b[0].is_ascii_digit() && b[1].is_ascii_digit() && b[2].is_ascii_alphabetic()
}

fn map_patch(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "beta 1.7" | "b1.7" => return Some("Beta 1.7".to_string()),
        "beta 1.8" | "b1.8" => return Some("Beta 1.8".to_string()),
        _ => {}
    }
    if let Some(rest) = raw.strip_prefix("1.") {
        let mut parts = rest.split('.');
        let minor: u32 = parts.next()?.parse().ok()?;
        let patch: Option<u32> = match parts.next() {
            Some(p) => Some(p.parse().ok()?),
            None => None,
        };
        if parts.next().is_some() {
            return None;
        }
        let family = match minor {
            0..=6 => format!("1.{minor}"),
            7 => "1.7.10".to_string(),
            8 => "1.8.9".to_string(),
            9 => "1.9.4".to_string(),
            10 => "1.10.2".to_string(),
            11 => "1.11.2".to_string(),
            12 => "1.12.2".to_string(),
            13 => "1.13.2".to_string(),
            14 => "1.14.4".to_string(),
            15 => "1.15.2".to_string(),
            16 => {
                if patch.is_some_and(|p| p <= 1) {
                    "1.16.1".to_string()
                } else {
                    "1.16.5".to_string()
                }
            }
            17 => "1.17.1".to_string(),
            18 => "1.18.2".to_string(),
            19 => {
                if patch.is_some_and(|p| p <= 2) {
                    "1.19.2".to_string()
                } else {
                    "1.19.4".to_string()
                }
            }
            20 => "1.20.6".to_string(),
            21 => match patch {
                None | Some(10..) => "1.21.11".to_string(),
                Some(0..=1) => "1.21.1".to_string(),
                Some(2..=3) => "1.21.3".to_string(),
                Some(4) => "1.21.4".to_string(),
                Some(5) => "1.21.5".to_string(),
                Some(6) => "1.21.6".to_string(),
                Some(7..=9) => "1.21.9".to_string(),
            },
            _ => return None,
        };
        return Some(family);
    }
    if let Some(rest) = raw.strip_prefix("26.") {
        let minor: u32 = rest.parse().ok()?;
        return match minor {
            1 => Some("26.1".to_string()),
            2 => Some("26.2".to_string()),
            3 => Some("26.3".to_string()),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_versions_resolve() {
        let r = resolve_version("26.3").unwrap();
        assert!(r.exact);
        assert_eq!(r.family, "26.3");
        assert_eq!(r.version, "26.3");
        assert!(supported_versions().len() >= 30);
    }

    #[test]
    fn patches_map_to_family() {
        assert_eq!(resolve_version("1.20.1").unwrap().family, "1.20.6");
        assert!(!resolve_version("1.20.1").unwrap().exact);
        assert_eq!(resolve_version("1.21.2").unwrap().family, "1.21.3");
        assert_eq!(resolve_version("1.21.7").unwrap().family, "1.21.9");
        assert_eq!(resolve_version("1.12").unwrap().family, "1.12.2");
        assert_eq!(resolve_version("1.0.0").unwrap().family, "1.0");
    }

    #[test]
    fn snapshots_rejected() {
        assert!(resolve_version("26.4-snapshot-5").is_err());
        assert!(resolve_version("1.21.2-pre1").is_err());
        assert!(resolve_version("26w14a").is_err());
    }

    #[test]
    fn unknown_versions_rejected() {
        assert!(resolve_version("1.22").is_err());
        assert!(resolve_version("").is_err());
    }
}
