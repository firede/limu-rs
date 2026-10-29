// limu's playground module, as a page calls it. The module exports a few functions, which
// pass text through its memory: the page writes a diagram's source where input() makes
// room, calls show(), and reads the JSON show() wrote at output(). A panic writes what it
// says there before the module traps, and the module is then started anew. The playground's
// page and limu's website both load the module through this script.

const views = { svg: 0, text: 1, scene: 2 };
const themes = { pele: 0, kohu: 1, 'kohu-dark': 2 };

// Loads the module from a URL, or from bytes already fetched, and gives the functions a page
// calls.
export async function load(wasm) {
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  const module = wasm instanceof URL || typeof wasm === 'string'
    ? await WebAssembly.compileStreaming(fetch(wasm)).catch(() => fetch(wasm).then((r) => r.arrayBuffer()).then((b) => WebAssembly.compile(b)))
    : await WebAssembly.compile(wasm);
  let limu;
  const start = () => {
    limu = new WebAssembly.Instance(module, {}).exports;
    limu.start();
  };
  start();
  const read = () => JSON.parse(dec.decode(new Uint8Array(limu.memory.buffer, limu.output(), limu.output_len())));
  const call = (f) => {
    try {
      f();
      return read();
    } catch (e) {
      if (!(e instanceof WebAssembly.RuntimeError)) throw e;
      let said;
      try { said = read(); } catch { said = {}; }
      start();
      return { error: `limu panicked: ${said.panic || e.message}\nThis is a bug in limu.`, line: 0, column: 0 };
    }
  };
  return {
    // Shows a diagram's source. The options are the view, "svg", "text" or "scene", the
    // theme, "pele", "kohu" or "kohu-dark", whether the SVG's colors are written as colors
    // rather than as the --limu-* variables, whether text is drawn in ASCII alone, and how
    // many columns text is laid out for. It gives the output, the diagram's type, its size
    // and its background, or the error, with its line, and its column in UTF-16 code units,
    // as a text area counts them, both 0 where it has no place.
    show(source, { view = 'svg', theme = 'kohu', literal = true, ascii = false, columns = 80 } = {}) {
      return call(() => {
        const b = enc.encode(source);
        // Making room may grow the memory, which leaves its old buffer detached, so the
        // buffer is taken after.
        const at = limu.input(b.length);
        new Uint8Array(limu.memory.buffer, at, b.length).set(b);
        limu.show(views[view] ?? 0, themes[theme] ?? 1, literal ? 1 : 0, ascii ? 1 : 0, columns, Date.now());
      });
    },
    // The sources limu-rs's tests draw, as pairs of a name and a source.
    samples() {
      return call(() => limu.samples());
    },
  };
}

// The source a link carries in its fragment, as #source=…, which is how Pele's playground
// and limu's website write it, or null.
export function linkedSource(hash = window.location.hash) {
  return hash.length > 1 ? new URLSearchParams(hash.slice(1)).get('source') : null;
}

// The fragment of a link that carries a source.
export function sourceHash(source) {
  return `#${new URLSearchParams({ source })}`;
}
