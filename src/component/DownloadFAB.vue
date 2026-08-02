<script setup lang="ts">
import { useRouter } from "vue-router"
import { dlState } from "../store/download"

const router = useRouter()
const R = 20
const C = 2 * Math.PI * R

function openDetail() {
  dlState.tasks.forEach(t => t.previousRoute = router.currentRoute.value.fullPath)
  router.push("/download-detail")
}
</script>

<template>
  <Transition name="fab-up">
    <button v-if="dlState.anyActive" class="fab" @click="openDetail">
      <svg class="fab-ring" viewBox="0 0 48 48" width="48" height="48">
        <circle cx="24" cy="24" :r="R" fill="none" stroke="var(--border-default)" stroke-width="3" />
        <circle cx="24" cy="24" :r="R" fill="none" stroke="var(--color-brand)" stroke-width="3"
          stroke-linecap="round"
          :stroke-dasharray="C"
          :stroke-dashoffset="C * (1 - dlState.totalProgress / 100)"
          transform="rotate(-90 24 24)"
          class="fab-progress"
        />
      </svg>
      <svg class="fab-icon" viewBox="0 0 16 16" width="18" height="18" fill="currentColor">
        <rect x="2" y="12" width="12" height="2" rx="1" />
        <polygon points="7,1 9,1 9,7 11.5,4.5 13,6 8,11 3,6 4.5,4.5 7,7" />
      </svg>
    </button>
  </Transition>
</template>

<style scoped>
.fab {
  position: fixed; z-index: 999;
  bottom: 24px; right: 24px;
  width: 52px; height: 52px;
  border: none; border-radius: 50%;
  background: var(--surface-card);
  box-shadow: var(--shadow-lg);
  cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: box-shadow 0.15s, transform 0.15s;
  color: var(--color-brand);
}
.fab:hover {
  box-shadow: 0 6px 24px rgba(99,102,241,0.3);
  transform: scale(1.05);
}
.fab:active { transform: scale(0.95); }
.fab-icon { position: absolute; pointer-events: none; }
.fab-ring { position: absolute; pointer-events: none; }
.fab-progress { transition: stroke-dashoffset 0.3s ease; }

.fab-up-enter-active { transition: all 0.25s ease-out; }
.fab-up-leave-active { transition: all 0.2s ease-in; }
.fab-up-enter-from { opacity: 0; transform: translateY(16px) scale(0.8); }
.fab-up-leave-to { opacity: 0; transform: translateY(16px) scale(0.8); }
</style>
