import { createApp } from "vue";
import App from "./App.vue";
import { Quasar, Notify } from "quasar";
import quasarIconSet from "quasar/icon-set/material-symbols-rounded";

import "@quasar/extras/roboto-font/roboto-font.css";
import "@quasar/extras/material-symbols-rounded/material-symbols-rounded.css";
import "quasar/src/css/index.sass";

const app = createApp(App);
app.use(Quasar, {
  config: {
    dark: true,
  },
  iconSet: quasarIconSet,
  // ... quasar config
  plugins: { Notify },
});
app.mount("#app");
