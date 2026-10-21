// Node host for the NetEase extension's existing Emscripten WASM and sandbox glue.
// The bundle is kept intact on disk; runtime adaptations provide its local WASM
// loader and expose its existing message listener.
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const readline = require("node:readline");

const bundlePath = path.join(__dirname, "sandbox.bundle.js");
let bundleSource = fs.readFileSync(bundlePath, "utf8");
const loaderAnchor = 'm=h?r(997).dirname(m)+"/":"//"';
const moduleAnchor = "var f=r(321),l=r.n(f);";
if (!bundleSource.includes(loaderAnchor) || !bundleSource.includes(moduleAnchor)) {
  throw new Error("Unexpected NetEase sandbox bundle layout");
}
bundleSource = bundleSource.replace(loaderAnchor, 'm=h?r(997).dirname(m)+"/":__dirname+"/"');
bundleSource = bundleSource.replace(moduleAnchor, `${moduleAnchor}globalThis.__neteaseFingerprintModule=l();`);

let messageListener;
global.crypto ||= require("node:crypto").webcrypto;
// Force Emscripten to use its Node fs loader for the local WASM path.
global.fetch = undefined;
global.fs = fs;
global.path = path;
global.window = {
  addEventListener(type, listener) {
    if (type === "message") messageListener = listener;
  },
};

const bundleModule = new Module(bundlePath, module);
bundleModule.filename = bundlePath;
bundleModule.paths = Module._nodeModulePaths(__dirname);
bundleModule._compile(bundleSource, bundlePath);

const wasmModule = global.__neteaseFingerprintModule;
if (!wasmModule) throw new Error("NetEase WASM module was not exposed by sandbox glue");
const ready = new Promise((resolve, reject) => {
  const previousAbort = wasmModule.onAbort;
  wasmModule.onAbort = (reason) => {
    if (previousAbort) previousAbort(reason);
    reject(new Error(`NetEase WASM aborted: ${reason}`));
  };
  wasmModule.onRuntimeInitialized = resolve;
  if (wasmModule.calledRun) resolve();
});

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on("line", async (line) => {
  try {
    await ready;
    if (!messageListener) throw new Error("Sandbox message handler was not registered");
    const request = JSON.parse(line);
    const pcm = Buffer.from(request.pcm_f32le, "base64");
    if (pcm.length !== 48_000 * 4) {
      throw new Error(`Expected 48000 f32 samples, got ${pcm.length / 4}`);
    }
    const samples = new Float32Array(48_000);
    for (let i = 0; i < samples.length; i += 1) samples[i] = pcm.readFloatLE(i * 4);

    // The extension resets its sandbox before each recognition attempt.
    messageListener({
      data: { type: "reset", data: "" },
      source: { postMessage() {} },
    });
    let result;
    messageListener({
      data: { type: "record", data: [samples] },
      source: {
        postMessage(message) {
          if (message.type === "recordCallBack") result = message.data;
        },
      },
    });
    if (!result || !(result.result instanceof ArrayBuffer)) {
      throw new Error("Sandbox did not return a fingerprint buffer");
    }
    const fingerprint = Buffer.from(new Uint8Array(result.result));
    process.stdout.write(`${JSON.stringify({
      fingerprint_b64: fingerprint.toString("base64"),
      times: result.times,
      duration: result.duration,
      session_id: result.sessionId,
    })}\n`);
  } catch (error) {
    process.stdout.write(`${JSON.stringify({ error: String(error?.stack || error) })}\n`);
  }
});
