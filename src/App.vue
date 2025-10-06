<template>
  <main class="app q-pa-lg">
    <q-tabs v-model="tab" class="q-mb-sm" active-color="secondary">
      <q-tab label="Map" name="map" />
      <q-tab label="Compute" name="compute" />
    </q-tabs>

    <map-page v-show="tab === 'map'" class="col-grow" />
    <compute-page v-show="tab === 'compute'" class="col-grow" />
  </main>
</template>

<script setup lang="ts">
import { ref } from "vue";
import ComputePage from "./pages/ComputePage.vue";
import MapPage from "./pages/MapPage.vue";

const tab = ref("map");
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
