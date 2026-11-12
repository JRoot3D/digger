// Starts the game (add-web-build-and-releases, design decision 10): imports
// the module wasm-bindgen generated, which runs the game's `main` once its
// program file has loaded. Any failure replaces the game area with a message
// that names the problem; the page is never left blank or frozen.

const root = document.documentElement;
let started = false;

function fail(text) {
  if (root.dataset.phase === "failed") return;
  root.dataset.phase = "failed";
  document.getElementById("loading")?.remove();
  document.getElementById("game")?.remove();
  document.getElementById("overlay")?.remove();
  const box = document.getElementById("failure");
  if (box) {
    box.textContent = text;
    box.hidden = false;
  }
}

const reason = (error) => (error && error.message) || String(error);

// After start-up, an exception that reaches the page is an internal error
// (with `panic = "abort"` a Rust panic arrives as one, after the panic hook
// has written its message to the console). Events without an error object,
// such as the browser's own resize-observer reports, are not.
addEventListener("error", (event) => {
  if (event.error == null) return;
  fail(started
    ? `The game stopped because of an internal error: ${reason(event.error)}. Reload the page to start again.`
    : `The game could not be loaded: ${reason(event.error)}.`);
});
addEventListener("unhandledrejection", (event) => {
  fail(`The game stopped because of an internal error: ${reason(event.reason)}. Reload the page to start again.`);
});

if (typeof WebAssembly !== "object") {
  fail("This browser cannot run the game: it does not support WebAssembly.");
} else {
  try {
    const { default: init } = await import("./digger.js");
    const program = new URL("digger_bg.wasm", import.meta.url);
    const response = await fetch(program);
    if (!response.ok) {
      throw new Error(`its program file ${program.pathname.split("/").pop()} could not be fetched (HTTP ${response.status})`);
    }
    await init({ module_or_path: response });
    started = true;
    // The game draws its first picture in the next animation callback.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      document.getElementById("loading")?.remove();
    }));
  } catch (error) {
    fail(`The game could not be loaded: ${reason(error)}.`);
  }
}
