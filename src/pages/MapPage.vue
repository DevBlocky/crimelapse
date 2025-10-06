<template>
  <div class="column">
    <FileFolderInput v-model="filepath" label="Exported Data (output.json)" />
    <LMap
      class="col-grow"
      v-model:zoom="mapZoom"
      v-model:center="mapCenter as any"
      :use-global-leaflet="false"
    >
      <LTileLayer
        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        layer-type="base"
        name="OpenStreetMap"
      />
      <LPolyline :lat-lngs="mapPolyline" :weight="2" :opacity="0.9" />
      <LMarker
        v-for="m in mapMarkers"
        :key="m.id"
        :lat-lng="m.latLng"
        @popupopen="onPopupOpen($event, m.id)"
        @popupclose="onPopupClose(m.id)"
      >
        <LPopup :options="popupOptions">
          <div class="q-pa-sm">
            <video
              v-if="openedPopupId === m.id"
              class="popup-video q-mb-sm"
              :src="m.loadVideoSrc()"
              controls
              preload="metadata"
            ></video>
            <pre class="q-mb-sm popup-pre">{{ m.data }}</pre>
          </div>
        </LPopup>
      </LMarker>
    </LMap>
  </div>
</template>

<script setup lang="ts">
import "leaflet/dist/leaflet.css";
import { Popup as LeafletPopup, LatLng, type PopupOptions } from "leaflet";
import {
  LMap,
  LTileLayer,
  LMarker,
  LPolyline,
  LPopup,
} from "@vue-leaflet/vue-leaflet";
import FileFolderInput from "../components/FileFolderInput.vue";
import { computed, nextTick, ref, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

interface ExportEntry {
  filePath: string;
  timestamp: string;
  duration: number;
  location: { lat: number; lng: number } | null;
}
const filepath = ref("");
const crimelapse = ref<ExportEntry[]>([]);
watch(
  () => filepath.value,
  async (filepath) => {
    const contents = await invoke<string>("read_file", { filepath });
    crimelapse.value = JSON.parse(contents);
  }
);

const mapZoom = ref(4);
const mapCenter = ref<LatLng>(new LatLng(39, -100));
const mapPolyline = computed(() => {
  return crimelapse.value
    .filter((x) => x.location && (x.location.lat || x.location.lng))
    .map(({ location }) => new LatLng(location!.lat, location!.lng));
});
interface MapMarker {
  id: string;
  latLng: LatLng;
  filePath: string;
  data: ExportEntry;
  loadVideoSrc: () => string;
}

const mapMarkers = computed<MapMarker[]>(() => {
  if (mapZoom.value < 11) return [];

  const closeEntries = crimelapse.value.filter((x) => {
    const latDst = Math.abs((x.location?.lat ?? 0) - mapCenter.value.lat);
    const lngDst = Math.abs((x.location?.lng ?? 0) - mapCenter.value.lng);
    return Math.max(latDst, lngDst) < 0.5;
  });
  return closeEntries.map((x) => {
    const latLng = new LatLng(x.location!.lat, x.location!.lng);
    return {
      id: x.filePath,
      latLng,
      filePath: x.filePath,
      data: x,
      loadVideoSrc: () => convertFileSrc(x.filePath),
    };
  });
});

const openedPopupId = ref<string | null>(null);
const onPopupOpen = (e: { popup: LeafletPopup }, id: string) => {
  openedPopupId.value = id;
  console.log(e.popup);
  nextTick(() => e.popup.update());
};
const onPopupClose = (id: string) => {
  if (openedPopupId.value === id) openedPopupId.value = null;
};

const popupOptions: PopupOptions = {
  autoPan: true,
  keepInView: true,
  maxWidth: 600,
  minWidth: 320,
  className: "map-popup",
};
</script>

<style scoped>
.popup-pre {
  white-space: pre-wrap;
}

:deep(.map-popup .leaflet-popup-content) {
  width: min(600px, 100%);
}

.popup-video {
  display: block;
  width: 100%;
  max-width: 100%;
  border-radius: 4px;
  background: #000;
}

:deep(.map-popup .leaflet-popup-content-wrapper) {
  display: flex;
  flex-direction: column;
}
</style>
