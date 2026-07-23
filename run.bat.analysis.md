# run.bat 命令拆解分析

## 完整命令（还原后）

```bash
"C:\Users\fish\.jdks\openjdk-ea-25\bin\java.exe"                     ← Java 路径（用户选的 JDK）
  -XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump
                                                                     ← PCL2 特定，防 Intel 显卡崩溃
  --sun-misc-unsafe-memory-access=allow                               ← JDK 22+ 兼容旧版 Unsafe
  --enable-native-access=ALL-UNNAMED                                 ← JDK 22+ 模块化
  "-Djava.library.path=...26.2-Fabric 0.19.3-natives/java"           ← natives 提取路径（启动器生成）
  "-Djna.tmpdir=...26.2-Fabric 0.19.3-natives/jna"                   ← JNA 临时目录
  "-Dorg.lwjgl.system.SharedLibraryExtractPath=...natives/lwjgl"     ← LWJGL 库
  "-Dio.netty.native.workdir=...natives/netty"                       ← Netty 本地库
  -Dminecraft.launcher.brand=PCL                                     ← 启动器标识
  -Dminecraft.launcher.version=406                                   ← 启动器版本
  -cp
    "D:\Minecraft\.minecraft\libraries\...\lz4-java-1.10.1.jar;      ← classpath：所有 library jar
     D:\Minecraft\.minecraft\libraries\...\gson-2.14.0.jar;           ← 从版本 json 的 libraries[].path 拼接
     D:\Minecraft\.minecraft\libraries\...\guava-33.6.0-jre.jar;
     ...共约 60+ jar...;
     D:\Minecraft\.minecraft\versions\26.2-Fabric 0.19.3\26.2-Fabric 0.19.3.jar"
                                                                     ← client.jar（版本 jar）
  "-DFabricMcEmu=net.minecraft.client.main.Main"                     ← Fabric 注入：指示真实主类
  -XX:-OmitStackTraceInFastThrow                                     ← 用户自定义 JVM 参数（从配置取）
  -Djdk.lang.Process.allowAmbiguousCommands=True
  -Dfml.ignoreInvalidMinecraftCertificates=True
  -Dfml.ignorePatchDiscrepancies=True
  -Xmx1024m                                                          ← 用户设置的内存（-Xms 未设）
  -XX:+UnlockExperimentalVMOptions                                   ← GC 调优参数（用户选"调优 G1GC"）
  -XX:+UseCompactObjectHeaders
  -XX:+UseG1GC
  -XX:G1NewSizePercent=20
  -XX:G1ReservePercent=20
  -XX:G1HeapRegionSize=32M
  -XX:MaxGCPauseMillis=50
  -XX:+PerfDisableSharedMem
  -XX:MinHeapFreeRatio=25
  -XX:MaxHeapFreeRatio=40
  -Dlog4j2.formatMsgNoLookups=true                                   ← Log4j 安全
  -Dstdout.encoding=UTF-8                                            ← 编码
  -Dstderr.encoding=UTF-8
  -Dfile.encoding=COMPAT
  net.fabricmc.loader.impl.launch.knot.KnotClient                    ← mainClass（从版本 json 取）
  --username BridgeFishDev                                            ← 游戏参数（替换模板 ${auth_player_name}）
  --version "26.2-Fabric 0.19.3"                                     ← ${version_name}
  --gameDir "D:\Minecraft\.minecraft\versions\26.2-Fabric 0.19.3"    ← ${game_directory}
  --assetsDir D:\Minecraft\.minecraft\assets                          ← ${assets_root}
  --assetIndex 32                                                     ← ${assets_index_name}
  --uuid 00000FFFFFFFFFFFFFFFFFFFFFF96436                             ← ${auth_uuid}
  --accessToken 00000FFFFFFFFFFFFFFFFFFFFFF96436                      ← ${auth_access_token}
  --clientId ${clientid}                                              ← ${clientid}
  --xuid ${auth_xuid}                                                 ← ${auth_xuid}
  --versionType PCL                                                   ← ${version_type}
  --width 854 --height 480                                            ← 窗口尺寸（用户配置）
```

## 参数来源分类

### 来源 A：版本 json（启动器解析生成）

| 命令片段 | json 字段 |
|---------|----------|
| `net.fabricmc.loader.impl.launch.knot.KnotClient` | `mainClass` |
| `--version "26.2-Fabric 0.19.3"` | 模板 `${version_name}` |
| `--assetIndex 32` | `assetIndex.id` |
| classpath 所有 jar | `libraries[].downloads.artifact.path` + `client.jar` |
| `"-DFabricMcEmu=..."` | Fabric 在 json 里注入的规则 |

### 来源 B：用户配置（存在 Settings.toml）

| 命令片段 | LaunchConfig 字段 |
|---------|-----------------|
| `"C:\Users\fish\.jdks\...\java.exe"` | Java 路径（JDK 检测选了之后存下） |
| `-Xmx1024m` | `memory_mode` |
| `-XX:+UseG1GC ...` 整串 | `gc_type`（TunedG1GC） |
| `-XX:-OmitStackTraceInFastThrow ...` | `jvm_args` |
| `--width 854 --height 480` | `window_width / window_height` |

### 来源 C：启动器生成（代码内写死的逻辑）

| 命令片段 | 说明 |
|---------|------|
| `-Djava.library.path=...natives/java` | 根据版本名 + 游戏目录拼出 natives 路径 |
| `-Dminecraft.launcher.brand=Elytra` | 启动器品牌标识 |
| `--sun-misc-unsafe-memory-access=allow` | 检测 JDK 版本后按需添加 |
| `${auth_player_name}` 的替换值 | 启动器填用户输入的用户名 |
| `${assets_root}` 的替换值 | `.minecraft/assets` |

## 参数顺序规范

```
java.exe
  <JVM 前置参数>          ← 系统兼容、module flags
  <系统属性 -D>            ← natives、编码、品牌
  -cp <classpath>         ← libraries + client.jar
  <JVM 调优参数>           ← GC、内存、自定义 jvm_args
  <mainClass>             ← 从版本 json 取
  <游戏参数>               ← 版本模板替换 + 窗口尺寸
```
