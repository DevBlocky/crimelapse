<template>
  <div class="progress-wrapper">
    <div class="row justify-center items-center">
      <q-btn class="q-mr-sm" label="Stop" color="negative" @click="emit('stop')" />
      <q-linear-progress class="col" :value="linearProgress" :indeterminate="total === 0" instant-feedback />
    </div>
    <div class="progress-panel q-mt-md">
      <q-input ref="detailsInput" :model-value="joinedDetails" class="progress-textarea" type="textarea" readonly
        :input-style="{ height: '100%', overflow: 'auto', whiteSpace: 'pre' }" :input-attrs="{ wrap: 'off' }" />
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  watch,
} from "vue";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { QInput } from "quasar";

interface ProgressPayload {
  progress?: number;
  progressInc?: number;
  total?: number;
  detail?: string;
}

const props = defineProps<{ jobId: unknown }>();
const emit = defineEmits<{ (e: "stop"): void }>();

const PROGRESS_THROTTLE_MS = 100;
const completed = ref(0);
const total = ref(0);
const details = ref<string[]>([]);
const detailsInput = ref<QInput>();

const joinedDetails = computed(() =>
  details.value.join("\n")
);

let flushHandle: number | null = null;
let progressUnlisten: UnlistenFn | null = null;
let pendingCompleted: number | null = null;
let pendingIncrement = 0;
let pendingTotal: number | null = null;
let pendingDetailIndex: number = 0;
const pendingDetails: string[] = [];

const linearProgress = computed(() => {
  if (total.value === 0) return 0;
  return completed.value / total.value;
});

function cancelFlushTimer() {
  if (flushHandle !== null) {
    window.clearTimeout(flushHandle);
    flushHandle = null;
  }
}

function resetPending() {
  pendingCompleted = null;
  pendingIncrement = 0;
  pendingTotal = null;
  pendingDetailIndex = 0;
  pendingDetails.length = 0;
}

function clearDisplayedProgress() {
  completed.value = 0;
  total.value = 0;
  details.value = [];
}

function flushProgressUpdates() {
  if (pendingTotal !== null) {
    total.value = pendingTotal;
    pendingTotal = null;
  }

  if (pendingCompleted !== null) {
    completed.value = pendingCompleted;
    pendingCompleted = null;
  }

  if (pendingIncrement !== 0) {
    completed.value += pendingIncrement;
    pendingIncrement = 0;
  }

  if (pendingDetails.length === 0) return;

  details.value.push(...pendingDetails);
  pendingDetails.length = 0;

  if (details.value.length > 10500)
    details.value = details.value.slice(details.value.length - 10000);

  nextTick(() => {
    const textarea = detailsInput.value?.getNativeElement();
    if (textarea) textarea.scrollTop = textarea.scrollHeight;
  });
}

function scheduleProgressFlush() {
  if (flushHandle !== null) {
    return;
  }

  flushHandle = window.setTimeout(() => {
    flushHandle = null;
    flushProgressUpdates();
  }, PROGRESS_THROTTLE_MS);
}

async function startListening(jobId: unknown) {
  clearDisplayedProgress();
  resetPending();
  cancelFlushTimer();

  progressUnlisten?.();
  progressUnlisten = await listen<ProgressPayload>(`progress:${jobId}`, (e) => {
    console.log('receive progress');
    const payload = e.payload;
    if (typeof payload.progress === "number") {
      pendingCompleted = payload.progress;
    }
    if (typeof payload.progressInc === "number") {
      pendingIncrement += payload.progressInc;
    }
    if (typeof payload.total === "number") {
      pendingTotal = payload.total;
    }
    if (payload.detail) {
      pendingDetailIndex++;
      pendingDetails.push(`[${pendingDetailIndex}] ${payload.detail}`);
    }

    scheduleProgressFlush();
  });
}

watch(
  () => props.jobId,
  (jobId) => startListening(jobId),
  { immediate: true }
);

onBeforeUnmount(() => {
  // cleanup
  cancelFlushTimer();
  progressUnlisten?.();
  progressUnlisten = null;
});
</script>

<style scoped>
.progress-wrapper {
  display: flex;
  flex-direction: column;
}

.progress-panel {
  flex: 1 1 auto;
  display: flex;
  min-height: 0;
}

.progress-textarea {
  flex: 1 1 auto;
  display: flex;
}

:deep(.progress-textarea .q-field__control) {
  height: 100%;
}

:deep(.progress-textarea .q-field__native) {
  height: 100%;
}

:deep(.progress-textarea textarea) {
  height: 100%;
  min-height: 0;
  resize: none;
  font-family: monospace;
  overflow: auto;
  white-space: pre;
}
</style>
