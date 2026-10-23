<script setup lang="ts">
// 设置 → 关于：应用信息、同类项目推荐、数据与镜像、ModLoader、许可与声明。
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Copy, ExternalLink } from "@lucide/vue";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import pcl2Icon from "../../assets/thanks/pcl2.png";
import hmclIcon from "../../assets/thanks/hmcl.png";
import mcimIcon from "../../assets/thanks/mcim.png";
import mcmodIcon from "../../assets/thanks/mcmod.png";
import bmclapiIcon from "../../assets/thanks/bmclapi.png";
import modrinthIcon from "../../assets/thanks/modrinth.png";
import curseforgeIcon from "../../assets/thanks/curseforge.svg";
import easytierIcon from "../../assets/thanks/easytier.png";
import fabricIcon from "../../assets/Fabric.png";
import quiltIcon from "../../assets/quilt.png";
import forgeIcon from "../../assets/forge.png";
import neoforgeIcon from "../../assets/neoForge.png";
import optifineIcon from "../../assets/optifined.png";
import mojangIcon from "../../assets/thanks/minecraft.png";
import azulIcon from "../../assets/thanks/azul.png";

const { t } = useI18n();

const REPO = "https://github.com/nefelibetas/Elytra";
const ISSUES = `${REPO}/issues`;
const RELEASES = `${REPO}/releases`;
const LICENSE = `${REPO}/blob/main/LICENSE`;
const THIRD_PARTY = `${REPO}/blob/main/THIRD-PARTY-NOTICES.md`;
const SECURITY = `${REPO}/blob/main/SECURITY.md`;
const PCL2_LICENSE = "https://github.com/Meloong-Git/PCL/blob/ade6e04c6eab65e945b0fc51faae1ba8e714245c/LICENCE";

const appIcon = "/icon.png";

interface CreditLink {
  labelKey: string;
  url: string;
}

interface FeatureCredit {
  name: string;
  icon: string;
  descKey: string;
  recommendKey: string;
  noteKey: string;
  links: CreditLink[];
  featured?: boolean;
}

interface Credit {
  name: string;
  nameKey?: string;
  icon: string;
  descKey: string;
  url: string;
}

const pcl2: FeatureCredit = {
  name: "PCL2",
  icon: pcl2Icon,
  descKey: "settings.about.credits.pcl2.desc",
  recommendKey: "settings.about.credits.pcl2.recommend",
  noteKey: "settings.about.credits.pcl2.note",
  featured: true,
  links: [
    { labelKey: "settings.about.credits.pcl2.repo", url: "https://github.com/Meloong-Git/PCL" },
    { labelKey: "settings.about.credits.pcl2.license", url: PCL2_LICENSE },
  ],
};

const hmcl: FeatureCredit = {
  name: "HMCL",
  icon: hmclIcon,
  descKey: "settings.about.credits.hmcl.desc",
  recommendKey: "settings.about.credits.hmcl.recommend",
  noteKey: "settings.about.credits.hmcl.note",
  links: [
    { labelKey: "settings.about.credits.hmcl.repo", url: "https://github.com/HMCL-dev/HMCL" },
    { labelKey: "settings.about.credits.hmcl.download", url: "https://github.com/HMCL-dev/HMCL/releases" },
  ],
};

const dataCredits: Credit[] = [
  { name: "MCIM", icon: mcimIcon, descKey: "settings.about.credits.mcim", url: "https://www.mcimirror.top/" },
  { name: "MCMod.cn", nameKey: "settings.about.credits.mcmodName", icon: mcmodIcon, descKey: "settings.about.credits.mcmod", url: "https://www.mcmod.cn/" },
  { name: "BMCLAPI", icon: bmclapiIcon, descKey: "settings.about.credits.bmclapi", url: "https://bmclapi2.bangbang93.com/" },
  { name: "Modrinth", icon: modrinthIcon, descKey: "settings.about.credits.modrinth", url: "https://modrinth.com/" },
  { name: "CurseForge", icon: curseforgeIcon, descKey: "settings.about.credits.curseforge", url: "https://www.curseforge.com/minecraft" },
];

const networkingCredits: Credit[] = [
  { name: "EasyTier", icon: easytierIcon, descKey: "settings.about.credits.easytier", url: "https://github.com/EasyTier/EasyTier" },
];

const loaderCredits: Credit[] = [
  { name: "Fabric", icon: fabricIcon, descKey: "settings.about.credits.fabric", url: "https://fabricmc.net/" },
  { name: "Quilt", icon: quiltIcon, descKey: "settings.about.credits.quilt", url: "https://quiltmc.org/" },
  { name: "Forge", icon: forgeIcon, descKey: "settings.about.credits.forge", url: "https://files.minecraftforge.net/" },
  { name: "NeoForge", icon: neoforgeIcon, descKey: "settings.about.credits.neoforge", url: "https://neoforged.net/" },
  { name: "OptiFine", icon: optifineIcon, descKey: "settings.about.credits.optifine", url: "https://optifine.net/" },
];

const runtimeCredits: Credit[] = [
  { name: "Mojang", icon: mojangIcon, descKey: "settings.about.credits.mojang", url: "https://www.minecraft.net/" },
  { name: "Azul", icon: azulIcon, descKey: "settings.about.credits.azul", url: "https://www.azul.com/downloads/" },
];

const version = ref("—");

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    // 开发/无 Tauri 环境下保持占位，不阻塞 About 渲染
  }
});

function open(url: string) {
  void openUrl(url).catch(e => showToast(errMsg(e)));
}

function copyFallback(text: string) {
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.setAttribute("readonly", "");
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  ta.style.pointerEvents = "none";
  document.body.appendChild(ta);
  ta.select();
  const ok = document.execCommand("copy");
  ta.remove();
  if (!ok) throw new Error("copy failed");
}

async function copyVersionInfo() {
  const info = [`Elytra Launcher ${version.value}`, navigator.userAgent].join("\n");
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(info);
    } else {
      copyFallback(info);
    }
    showToast(t("settings.about.copied"));
  } catch {
    try {
      copyFallback(info);
      showToast(t("settings.about.copied"));
    } catch {
      showToast(t("settings.about.copyFailed"));
    }
  }
}
</script>

<template>
  <div class="about">
    <header class="ab-hero">
      <img class="ab-app-icon" :src="appIcon" alt="" />
      <div class="ab-hero-main">
        <div class="ab-title-row">
          <h2>Elytra Launcher</h2>
          <span class="ab-version">{{ t("settings.about.version", { version }) }}</span>
        </div>
        <p class="ab-tagline">{{ t("settings.about.tagline") }}</p>
        <div class="ab-quick">
          <button type="button" @click="open(REPO)">{{ t("settings.about.repo") }}</button>
          <button type="button" @click="open(ISSUES)">{{ t("settings.about.issues") }}</button>
          <button type="button" @click="open(RELEASES)">{{ t("settings.about.releases") }}</button>
          <button type="button" @click="copyVersionInfo">
            <Copy :size="13" />
            {{ t("settings.about.copyVersion") }}
          </button>
        </div>
      </div>
    </header>

    <section class="ab-section">
      <h3>{{ t("settings.about.specialThanks") }}</h3>
      <p class="ab-section-note">{{ t("settings.about.specialThanksNote") }}</p>

      <article class="ab-feature" :class="{ 'is-featured': pcl2.featured }">
        <div class="ab-feature-head">
          <span class="ab-feature-icon"><img :src="pcl2.icon" alt="" /></span>
          <div>
            <h4>{{ pcl2.name }}</h4>
            <p>{{ t(pcl2.descKey) }}</p>
          </div>
        </div>
        <p class="ab-recommend">{{ t(pcl2.recommendKey) }}</p>
        <p class="ab-note">{{ t(pcl2.noteKey) }}</p>
        <div class="ab-links">
          <button v-for="link in pcl2.links" :key="link.url" type="button" class="ab-link" @click="open(link.url)">
            {{ t(link.labelKey) }}
            <ExternalLink :size="11" />
          </button>
        </div>
      </article>

      <article class="ab-feature">
        <div class="ab-feature-head">
          <span class="ab-feature-icon"><img :src="hmcl.icon" alt="" /></span>
          <div>
            <h4>{{ hmcl.name }}</h4>
            <p>{{ t(hmcl.descKey) }}</p>
          </div>
        </div>
        <p class="ab-recommend">{{ t(hmcl.recommendKey) }}</p>
        <p class="ab-note">{{ t(hmcl.noteKey) }}</p>
        <div class="ab-links">
          <button v-for="link in hmcl.links" :key="link.url" type="button" class="ab-link" @click="open(link.url)">
            {{ t(link.labelKey) }}
            <ExternalLink :size="11" />
          </button>
        </div>
      </article>
    </section>

    <section class="ab-section">
      <h3>{{ t("settings.about.dataMirrors") }}</h3>
      <div class="ab-grid">
        <button v-for="c in dataCredits" :key="c.name" type="button" class="ab-credit" @click="open(c.url)">
          <span class="ab-credit-icon"><img :src="c.icon" alt="" /></span>
          <span class="ab-credit-body">
            <b>{{ c.nameKey ? t(c.nameKey) : c.name }}</b>
            <small>{{ t(c.descKey) }}</small>
          </span>
        </button>
      </div>
    </section>

    <section class="ab-section">
      <h3>{{ t("settings.about.networking") }}</h3>
      <div class="ab-grid">
        <button v-for="c in networkingCredits" :key="c.name" type="button" class="ab-credit" @click="open(c.url)">
          <span class="ab-credit-icon"><img :src="c.icon" alt="" /></span>
          <span class="ab-credit-body">
            <b>{{ c.nameKey ? t(c.nameKey) : c.name }}</b>
            <small>{{ t(c.descKey) }}</small>
          </span>
        </button>
      </div>
    </section>

    <section class="ab-section">
      <h3>{{ t("settings.about.loaders") }}</h3>
      <div class="ab-grid">
        <button v-for="c in loaderCredits" :key="c.name" type="button" class="ab-credit" @click="open(c.url)">
          <span class="ab-credit-icon"><img :src="c.icon" alt="" /></span>
          <span class="ab-credit-body">
            <b>{{ c.nameKey ? t(c.nameKey) : c.name }}</b>
            <small>{{ t(c.descKey) }}</small>
          </span>
        </button>
      </div>
    </section>

    <section class="ab-section">
      <h3>{{ t("settings.about.runtime") }}</h3>
      <div class="ab-grid">
        <button v-for="c in runtimeCredits" :key="c.name" type="button" class="ab-credit" @click="open(c.url)">
          <span class="ab-credit-icon"><img :src="c.icon" alt="" /></span>
          <span class="ab-credit-body">
            <b>{{ c.nameKey ? t(c.nameKey) : c.name }}</b>
            <small>{{ t(c.descKey) }}</small>
          </span>
        </button>
      </div>
    </section>

    <section class="ab-section ab-license">
      <h3>{{ t("settings.about.license.title") }}</h3>
      <div class="ab-license-row">
        <strong>{{ t("settings.about.license.gpl") }}</strong>
        <span>{{ t("settings.about.license.copyright") }}</span>
      </div>
      <div class="ab-links">
        <button type="button" class="ab-link" @click="open(LICENSE)">
          {{ t("settings.about.license.licenseLink") }}
          <ExternalLink :size="11" />
        </button>
        <button type="button" class="ab-link" @click="open(THIRD_PARTY)">
          {{ t("settings.about.license.thirdParty") }}
          <ExternalLink :size="11" />
        </button>
        <button type="button" class="ab-link" @click="open(SECURITY)">
          {{ t("settings.about.license.security") }}
          <ExternalLink :size="11" />
        </button>
      </div>
      <p class="ab-note">{{ t("settings.about.license.icons") }}</p>
      <p class="ab-note">{{ t("settings.about.license.disclaimer") }}</p>
    </section>
  </div>
</template>

<style scoped>
.about {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding-bottom: 8px;
}

.ab-hero {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 20px 24px;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
}

.ab-app-icon {
  width: 64px;
  height: 64px;
  flex: none;
  border-radius: 14px;
  object-fit: contain;
}

.ab-hero-main {
  min-width: 0;
}

.ab-title-row {
  display: flex;
  align-items: baseline;
  gap: 10px;
  flex-wrap: wrap;
}

.ab-title-row h2 {
  margin: 0;
  font-size: 20px;
  color: var(--text-primary);
}

.ab-version {
  padding: 2px 8px;
  font-size: 11.5px;
  color: var(--text-secondary);
  background: var(--surface-hover);
  border-radius: 999px;
}

.ab-tagline {
  margin: 4px 0 10px;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.ab-quick {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.ab-quick button {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.ab-quick button:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

.ab-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.ab-section > h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.ab-section-note {
  margin: -2px 0 2px;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.ab-feature {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 18px;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
}

.ab-feature.is-featured {
  border-color: var(--color-brand);
}

.ab-feature-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.ab-feature-icon {
  display: grid;
  place-items: center;
  width: 52px;
  height: 52px;
  flex: none;
  padding: 4px;
  overflow: hidden;
  background: var(--surface-elevated);
  border-radius: 12px;
}

.ab-feature.is-featured .ab-feature-icon {
  width: 60px;
  height: 60px;
}

.ab-feature-icon img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.ab-feature h4 {
  margin: 0 0 4px;
  font-size: 15px;
  color: var(--text-primary);
}

.ab-feature p {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.ab-recommend {
  padding: 8px 10px;
  color: var(--text-primary) !important;
  background: var(--surface-hover);
  border-radius: var(--radius-sm);
}

.ab-note {
  font-size: 11px !important;
  line-height: 1.5 !important;
  color: var(--text-tertiary) !important;
}

.ab-links {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.ab-link {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 0;
  font-size: 12px;
  color: var(--color-brand);
  background: none;
  border: none;
  cursor: pointer;
}

.ab-link:hover {
  text-decoration: underline;
}

.ab-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 8px;
}

.ab-credit {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  font-family: inherit;
  text-align: left;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: border-color 0.15s, background-color 0.15s;
}

.ab-credit:hover {
  background: var(--surface-hover);
  border-color: var(--color-brand);
}

.ab-credit-icon {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  flex: none;
  padding: 3px;
  overflow: hidden;
  background: var(--surface-elevated);
  border-radius: 8px;
}

.ab-credit-icon img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.ab-credit-body {
  min-width: 0;
}

.ab-credit-body b {
  display: block;
  font-size: 12.5px;
  color: var(--text-primary);
}

.ab-credit-body small {
  display: block;
  margin-top: 2px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-tertiary);
}

.ab-license {
  padding: 16px 18px;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
}

.ab-license-row {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: baseline;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.ab-license-row strong {
  color: var(--text-primary);
}
</style>
