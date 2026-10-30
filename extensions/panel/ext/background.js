// Firefox only wakes the background for listeners registered synchronously,
// so they are registered here; the work itself is in Rust (notify.rs).
import init, { start_watch, download_changed } from "./pkg/karakulak_panel.js";
const ready = init();
const watch = () => ready.then(start_watch);
browser.runtime.onStartup.addListener(watch);
browser.runtime.onInstalled.addListener(watch);
browser.alarms.onAlarm.addListener(watch);
browser.alarms.create("karakulak-watch", { periodInMinutes: 1 });
browser.downloads.onChanged.addListener((delta) => ready.then(() => download_changed(delta)));
