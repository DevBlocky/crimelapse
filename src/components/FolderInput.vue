<template>
  <q-input :model-value="modelValue" :label="label" readonly outlined>
    <template #append>
      <q-btn label="Choose..." color="grey-10" :disable="disable" @click="choosePath" />
    </template>
  </q-input>
</template>

<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";

const props = defineProps<{
  modelValue: string;
  label?: string;
  disable?: boolean;
  dialogTitle?: string;
  createDirectory?: boolean;
}>();
const emit = defineEmits<{
  (event: "update:modelValue", value: typeof props.modelValue): void;
}>();

async function choosePath() {
  const path = await open({
    title: props.dialogTitle,
    directory: true,
    canCreateDirectories: props.createDirectory,
  });
  if (path) emit("update:modelValue", path);
}
</script>
