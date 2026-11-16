// The only script on the page: it loads the editor, which is the Rust
// program compiled to WebAssembly, and says so plainly if that fails.
import init from "./arcs_editor.js";
const note = document.getElementById("note");
init().then(() => note.remove(), (error) => {
  // three-d runs its event loop by throwing this once on purpose
  if (String(error).includes("Using exceptions for control flow")) { note.remove(); return; }
  note.textContent = "The editor could not start: " + error + ". It needs a browser with WebGL 2.";
});
