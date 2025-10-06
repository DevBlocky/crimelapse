<template>
  <div class="column">
    <div class="inputs column">
      <folder-input
        v-model="inputPath"
        label="Clips Folder"
        dialog-title="Select Folder w/ Raw Clips"
        directory
      />
      <folder-input
        v-model="outputPath"
        class="q-mt-md"
        label="Output Folder"
        dialog-title="Select Output Folder"
        directory
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
      <div class="row items-start q-mt-sm">
        <q-checkbox v-model="exportOpts.enabled" label="Export Data" />
        <q-checkbox
          v-if="exportOpts.enabled"
          v-model="exportOpts.location"
          label="Scrape Geolocation (slow, but needed for map)"
        />
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
          (!timelapseOpts.enabled && !exportOpts.enabled)
        "
        @click="onStartJob"
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
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import FolderInput from "../components/FileFolderInput.vue";
import ProgressPanel from "../components/ProgressPanel.vue";
import { desktopDir, join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";
import { useQuasar } from "quasar";

const q = useQuasar();

const inputPath = ref("");
const outputPath = ref("");
const timelapseOpts = reactive({
  enabled: false,
  type: "mp4",
  fps: 30,
  length: 300,
  skip: 0,
});
const exportOpts = reactive({
  enabled: false,
  location: false,
});
const threads = ref(1);

const jobId = ref<unknown>();
const isWorking = ref(false);

async function onStartJob() {
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
    export: {
      enabled: exportOpts.enabled,
      location: exportOpts.location,
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
  invoke<number>("get_parallelism")
    .then((t) => (threads.value = Math.ceil(t / 2))) // use half of available
    .catch(console.error);
});
</script>

<style scoped>
.inputs {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
}
</style>
