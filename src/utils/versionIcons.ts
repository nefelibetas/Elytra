import grassIcon from "../assets/grass.png";
import forgeIcon from "../assets/forge.png";
import neoForgeIcon from "../assets/neoForge.png";
import fabricIcon from "../assets/Fabric.png";
import optifineIcon from "../assets/optifined.png";
import quiltIcon from "../assets/quilt.png";

/** 分类 key → 图标（VersionSelect 分类卡片 / VersionSetup 概览共用）。 */
export const CATEGORY_ICONS: Record<string, string> = {
  vanilla: grassIcon,
  forge: forgeIcon,
  neoforge: neoForgeIcon,
  fabric: fabricIcon,
  quilt: quiltIcon,
  optifine: optifineIcon,
  liteloader: "",
};
