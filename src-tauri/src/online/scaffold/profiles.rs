//! 玩家表：协议 `c:player_ping` / `c:player_profiles_list` 的数据模型与在线维护。
//!
//! host 侧维护 profiles（房主 HOST + 各 guest），收 ping 刷新时间戳、超时剔除；`player_profiles_list`
//! 返回 JSON 数组。guest 侧存自己看到的列表（host + 其他 guest + 本地 LOCAL）。
//! `kind`：HOST = 联机中心（房主）；GUEST = 房客；LOCAL = 本地玩家（仅客户端视图，不上报协议）。

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// 心跳超时：5s 心跳，10s 无包视为离线（参考 Terracotta 行为，协议未规定）。
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProfileKind {
    Host,
    Guest,
    /// 仅客户端本地视图（不上报协议）。
    Local,
}

impl ProfileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Host => "HOST",
            Self::Guest => "GUEST",
            Self::Local => "LOCAL",
        }
    }
}

/// 玩家档案（协议 JSON 字段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub machine_id: String,
    pub vendor: String,
    pub kind: ProfileKind,
}

impl Profile {
    pub fn new(name: String, machine_id: String, vendor: String, kind: ProfileKind) -> Self {
        Self {
            name,
            machine_id,
            vendor,
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OnlineProfile {
    pub profile: Profile,
    pub last_seen: SystemTime,
}

/// host 玩家表（串行访问，由调用方持锁）。
#[derive(Debug, Default, Clone)]
pub struct ProfileTable {
    /// 索引 0 恒为 HOST（房主），其余为 guest。
    entries: Vec<OnlineProfile>,
}

impl ProfileTable {
    pub fn with_host(host: Profile) -> Self {
        Self {
            entries: vec![OnlineProfile {
                profile: host,
                last_seen: SystemTime::now(),
            }],
        }
    }

    /// 收到 guest 心跳：存在（非 host 槽）则刷新时间/更新名字，否则插入；返回是否变化（插入或改名）。
    pub fn upsert_guest(&mut self, profile: Profile) -> bool {
        let now = SystemTime::now();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .skip(1)
            .find(|e| e.profile.machine_id == profile.machine_id)
        {
            let changed = entry.profile.name != profile.name || entry.profile.vendor != profile.vendor;
            entry.profile = profile;
            entry.last_seen = now;
            changed
        } else {
            self.entries.push(OnlineProfile {
                profile,
                last_seen: now,
            });
            true
        }
    }

    /// 剔除超时 guest（保留 HOST 槽）。返回是否剔除。
    pub fn purge_expired(&mut self) -> bool {
        let now = SystemTime::now();
        let before = self.entries.len();
        self.entries
            .retain(|e| e.profile.kind == ProfileKind::Host || now.duration_since(e.last_seen).map(|d| d < HEARTBEAT_TIMEOUT).unwrap_or(true));
        self.entries.len() != before
    }

    pub fn profiles(&self) -> Vec<Profile> {
        self.entries.iter().map(|e| e.profile.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, mid: &str) -> Profile {
        Profile::new(name.into(), mid.into(), "test".into(), ProfileKind::Guest)
    }

    #[test]
    fn host_table_starts_with_host() {
        let host = Profile::new("host".into(), "m0".into(), "v".into(), ProfileKind::Host);
        let t = ProfileTable::with_host(host.clone());
        assert_eq!(t.profiles(), vec![host]);
    }

    #[test]
    fn upsert_adds_and_refreshes() {
        let host = Profile::new("host".into(), "m0".into(), "v".into(), ProfileKind::Host);
        let mut t = ProfileTable::with_host(host);
        assert!(t.upsert_guest(profile("alice", "m1")));
        assert_eq!(t.profiles().len(), 2);
        assert!(!t.upsert_guest(profile("alice", "m1")));
        assert!(t.upsert_guest(profile("alice2", "m1")));
        let ps = t.profiles();
        assert_eq!(ps[1].name, "alice2");
        assert_eq!(ps.len(), 2, "不应重复插入");
    }

    #[test]
    fn purge_removes_expired_keeps_host() {
        let host = Profile::new("host".into(), "m0".into(), "v".into(), ProfileKind::Host);
        let mut t = ProfileTable::with_host(host);
        t.upsert_guest(profile("alice", "m1"));
        t.entries[1].last_seen = SystemTime::now() - Duration::from_secs(30);
        assert!(t.purge_expired());
        assert_eq!(t.profiles().len(), 1, "仅剩 host");
        assert_eq!(t.profiles()[0].kind, ProfileKind::Host);
        assert!(!t.purge_expired());
    }

    #[test]
    fn guest_cannot_overwrite_host() {
        let host = Profile::new("host".into(), "m0".into(), "v".into(), ProfileKind::Host);
        let mut t = ProfileTable::with_host(host);
        // guest 心跳带 host 的 machine_id（冲突）：upsert 只操作 skip(1)，不触碰 host 槽
        t.upsert_guest(profile("fake", "m0"));
        assert_eq!(t.profiles()[0].name, "host", "host 槽不可被覆盖");
        assert_eq!(t.profiles()[0].kind, ProfileKind::Host);
    }
}
