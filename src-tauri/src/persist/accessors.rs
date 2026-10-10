/// 宏分发实现if
macro_rules! default_of {
    ($ty: ty) => {
        <$ty as Default>::default()
    };
    ($ty: ty, $default: expr) => {
        $default
    };
}

/// 生成 section 的类型化读写函数（消除 `read::<T>(SECT).unwrap_or_default()` /
/// `save(SECT, &T)` 样板：section 常量只在调用宏处出现一次）。
macro_rules! section_accessors {
    ($($load:ident, $store:ident => $ty:ty, $sect:path $(, $default: expr)?;)*) => {
        $(
            #[doc = concat!("读取 `", stringify!($sect), "`(缺失/损坏时返回默认值)。")]
            pub fn $load() -> $ty {
                $crate::persist::read($sect)
                .unwrap_or_else(|_| default_of!($ty $(, $default)?))
            }

            #[doc = concat!("写入 `", stringify!($sect), "`。")]
            pub fn $store(data: &$ty) -> Result<(), $crate::persist::PersistError> {
                $crate::persist::save($sect, data)
            }
        )*
    };
}

section_accessors! {
    downloader_config, save_downloader_config => crate::config::downloader::DownloaderConfig, crate::constants::SECT_DOWNLOADER;
    game_dirs_config, save_game_dirs_config => crate::config::game_dirs::GameDirsConfig, crate::constants::SECT_GAME_DIRS;
    launch_options, save_launch_options => crate::launcher::model::LaunchOptions, crate::constants::SECT_LAUNCHER;
    tooldl_config, save_tooldl_config => crate::config::tooldl::ToolDlConfig, crate::constants::SECT_TOOL_DL;
    online_config, save_online_config => crate::config::online::OnlineConfig, crate::constants::SECT_ONLINE, crate::config::online::OnlineConfig::with_defaults();
}
