// Firefox only wakes the background for listeners registered synchronously,
// so they are registered here; the work itself is in Rust.
import init, { open_welcome_if_needed, set_level, get_level } from "./pkg/karakulak_welcome.js";
const ready = init();
const run = () => ready.then(open_welcome_if_needed);
browser.runtime.onInstalled.addListener(run);
browser.runtime.onStartup.addListener(run);

// The privacy level from Settings, only from Karakulak's own panel extension.
browser.runtime.onMessageExternal.addListener((message, sender) => {
  if (sender.id !== "panel@karakulak") {
    return undefined;
  }
  return ready.then(() => (message.level ? set_level(message.level) : get_level()));
});
