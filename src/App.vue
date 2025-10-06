<template>
  <main class="app q-pa-lg">
    <q-tabs v-model="tab" class="q-mb-sm" active-color="secondary">
      <q-tab label="Compute" name="compute" />
      <q-tab label="Map" name="map" />
    </q-tabs>

    <compute-page v-show="tab === 'compute'" class="col-grow" />
  </main>
</template>

<script setup lang="ts">
import { ref } from "vue";
import ComputePage from "./pages/ComputePage.vue";

const tab = ref("compute");
</script>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
.col-grow > * {
  height: 100%;
}
</style>
