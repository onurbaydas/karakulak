// Firefox only wakes the background for listeners registered synchronously,
// so they are registered here; the work itself is in Rust.
import init, { open_welcome_if_needed } from "./pkg/karakulak_welcome.js";
const ready = init();
const run = () => ready.then(open_welcome_if_needed);
browser.runtime.onInstalled.addListener(run);
browser.runtime.onStartup.addListener(run);
