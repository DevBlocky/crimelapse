<template>
  <main class="app q-pa-lg">
    <div class="inputs column">
      <folder-input
        v-model="inputPath"
        label="Clips Folder"
        dialog-title="Select Folder w/ Raw Clips"
      />
      <folder-input
        v-model="outputPath"
        class="q-mt-md"
        label="Output Folder"
        dialog-title="Select Output Folder"
        create-directory
      />
      <div class="row items-start q-mt-sm">
        <q-checkbox v-model="timelapseOpts.enabled" label="Timelapse" />
        <q-input
          v-if="timelapseOpts.enabled"
          v-model.number="timelapseOpts.fps"
          label="Frames Per Second"
          suffix="fps"
          color="accent"
          filled
          dense
        />
        <q-input
          v-if="timelapseOpts.enabled"
          v-model.number="timelapseOpts.length"
          label="Length"
          suffix="seconds"
          color="accent"
          filled
          dense
        />
        <q-input
          v-if="timelapseOpts.enabled"
          v-model.number="timelapseOpts.skip"
          label="Skip Frames"
          color="accent"
          filled
          dense
        />
        <q-btn-toggle
          v-if="timelapseOpts.enabled"
          v-model="timelapseOpts.type"
          :options="[
            { label: 'jpg', value: 'jpg' },
            { label: 'mp4', value: 'mp4' },
          ]"
          toggle-color="accent"
        />
      </div>
      <div class="q-mt-sm">
        <q-checkbox v-model="geoOpts.enabled" label="Geolocation JSON" disable />
      </div>
    </div>

    <div v-if="!isWorking" class="q-mt-md row">
      <q-btn
        v-if="!isWorking"
        label="Process"
        size="lg"
        color="primary"
        :disable="
          !inputPath ||
          threads < 1 ||
          (!timelapseOpts.enabled && !geoOpts.enabled)
        "
        @click="beginProcessing"
      />
      <q-input
        v-model.number="threads"
        class="q-ml-sm"
        label="# Threads"
        filled
      />
    </div>
    <progress-panel
      v-else
      class="col q-mt-md"
      :job-id="jobId"
      @stop="onStopJob"
    />
  </main>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import FolderInput from "./components/FolderInput.vue";
import ProgressPanel from "./components/ProgressPanel.vue";
import { desktopDir, join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";
import { useQuasar } from "quasar";

const q = useQuasar();

const inputPath = ref("");
const outputPath = ref("");
const timelapseOpts = reactive({
  enabled: true,
  type: "mp4",
  fps: 30,
  length: 300,
  skip: 0,
});
const geoOpts = reactive({
  enabled: false,
});
const threads = ref(1);

const jobId = ref<unknown>();
const isWorking = ref(false);

async function beginProcessing() {
  isWorking.value = true;
  jobId.value = await invoke("start_job", {
    threads: threads.value,
    inputPath: inputPath.value,
    outputPath: outputPath.value,

    timelapse: {
      typ: timelapseOpts.enabled ? timelapseOpts.type : "none",
      length: timelapseOpts.length,
      fps: timelapseOpts.fps,
      skip: timelapseOpts.skip,
    },
  });
}
async function onStopJob() {
  isWorking.value = false;
  const success = await invoke<boolean>("cancel_job", {
    jobId: jobId.value,
  });
  jobId.value = null;
  console.log("cancel success?", success);
  q.notify({
    message: "Job cancelled",
    color: success ? "positive" : "negative",
  });
}

onMounted(() => {
  desktopDir()
    .then((desktopPath) => join(desktopPath, "crimelapse"))
    .then((path) => (outputPath.value = path))
    .catch(console.error);
  invoke<number>("get_parallelism").then(
    (t) => (threads.value = Math.ceil(t / 2))
  );
});
</script>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}

.inputs {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
}
</style>
