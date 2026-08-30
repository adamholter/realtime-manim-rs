import init, {
  create_player,
} from "./pkg/realtime_manim_web_preview.js";
import { ArrayBufferTarget, Muxer } from "/vendor/webm-muxer.mjs";
import {
  DEFAULT_SCENE,
  formatSceneCode,
  parseSceneCode,
  validateScene,
} from "./scene-schema.js";

const byId = (id) => document.querySelector(`#${id}`);
const elements = {
  apiKey: byId("api-key"),
  assetInput: byId("asset-input"),
  assetStatus: byId("asset-status"),
  codeError: byId("code-error"),
  codeOutput: byId("code-output"),
  codeShell: byId("code-shell"),
  codeStatus: byId("code-status"),
  compileTime: byId("compile-time"),
  connectButton: byId("connect-button"),
  copyButton: byId("copy-button"),
  captionDownloadLink: byId("caption-download-link"),
  disconnectButton: byId("disconnect-button"),
  downloadLink: byId("download-link"),
  generateButton: byId("generate-button"),
  keyForm: byId("key-form"),
  keyStatus: byId("key-status"),
  modelTime: byId("model-time"),
  outputVideo: byId("output-video"),
  pauseButton: byId("pause-button"),
  prompt: byId("prompt"),
  promptForm: byId("prompt-form"),
  promptStatus: byId("prompt-status"),
  renderStatus: byId("render-status"),
  renderTime: byId("render-time"),
  runButton: byId("run-button"),
  sceneControlPanel: byId("scene-control-panel"),
  sceneControls: byId("scene-controls"),
  sceneTitle: byId("scene-title"),
  streamCaret: byId("stream-caret"),
  totalTime: byId("total-time"),
  timelineControl: byId("timeline-control"),
  timelineOutput: byId("timeline-output"),
  transcript: byId("transcript"),
  videoDescription: byId("video-description"),
  videoOutput: byId("video-output"),
  videoSize: byId("video-size"),
  viewport: byId("viewport"),
};

let wasmReady = false;
let webPlayer = null;
let connected = false;
let generating = false;
let paused = false;
let timelineScrubbing = false;
let timelineSceneControl = null;
let currentCode = formatSceneCode(DEFAULT_SCENE);
let currentScene = DEFAULT_SCENE;
let activeScene = DEFAULT_SCENE;
let currentVideoUrl;
let currentCaptionUrl;
let latestModelMs = 0;
let latestCompileMs = 0;
const typesetRequests = new Map();

const player = () => {
  if (!webPlayer || webPlayer.is_destroyed()) throw new Error("The Rust renderer is unavailable.");
  return webPlayer;
};
const clear_render_size = () => player().clear_render_size();
const current_scene_time = () => player().current_time();
const load_scene = (scene) => player().load_scene(scene);
const reset_clock = () => player().reset_clock();
const resume_scene = () => player().resume();
const seek_scene = (time) => player().seek(time);
const set_render_size = (width, height) => player().set_render_size(width, height);
const set_paused = (paused) => player().set_paused(paused);
const set_signal = (signal, value) => player().set_signal(signal, value);

export function previewDiagnostics() {
  const renderer = player();
  let time = null;
  let engineAvailable = true;
  try {
    time = renderer.current_time();
  } catch {
    engineAvailable = false;
  }
  return {
    recoveryCount: renderer.recovery_count(),
    presentedFrames: renderer.presented_frame_count(),
    skippedFrames: renderer.skipped_frame_count(),
    lastCpuFrameMs: renderer.last_cpu_frame_ms(),
    time,
    engineAvailable,
    destroyed: renderer.is_destroyed(),
  };
}

export function seekPreview(time) {
  set_paused(true);
  seek_scene(time);
  return current_scene_time();
}

export function simulatePreviewDeviceLoss() {
  player().simulate_device_loss();
}

window.addEventListener("pagehide", () => {
  webPlayer?.destroy();
  webPlayer = null;
}, { once: true });

function seconds(milliseconds) {
  return `${(milliseconds / 1000).toFixed(2)} s`;
}

function bytes(size) {
  if (!Number.isFinite(size)) return "—";
  if (size < 1024 * 1024) return `${Math.max(1, Math.round(size / 1024))} KB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

function setConnected(nextConnected) {
  connected = nextConnected;
  elements.keyForm.hidden = nextConnected;
  elements.disconnectButton.hidden = !nextConnected;
  elements.generateButton.disabled = !nextConnected || generating;
  elements.assetInput.disabled = !nextConnected || generating;
  elements.promptStatus.textContent = nextConnected
    ? "Terra connected"
    : "Connect a key to generate";
  elements.keyStatus.textContent = nextConnected ? "Connected to GPT-5.6 Terra" : "Not connected";
  elements.keyStatus.classList.toggle("form-status--success", nextConnected);
}

function setGenerating(nextGenerating) {
  generating = nextGenerating;
  elements.generateButton.disabled = !connected || nextGenerating;
  elements.connectButton.disabled = nextGenerating;
  elements.runButton.disabled = nextGenerating || !wasmReady;
  elements.assetInput.disabled = !connected || nextGenerating;
  elements.streamCaret.hidden = !nextGenerating;
  elements.codeShell.classList.toggle("code-shell--streaming", nextGenerating);
  elements.promptStatus.textContent = nextGenerating ? "Terra is writing…" : connected ? "Terra connected" : "Connect a key to generate";
}

function showCodeError(message) {
  elements.codeError.textContent = message;
  elements.codeError.hidden = !message;
}

function renderCode(code) {
  currentCode = code;
  elements.codeOutput.textContent = code;
  elements.codeShell.scrollTop = elements.codeShell.scrollHeight;
}

function addMessage(role, content, detail = "") {
  const article = document.createElement("article");
  article.className = `message message--${role}`;
  const label = document.createElement("span");
  label.textContent = role === "user" ? "You" : "Lab";
  const paragraph = document.createElement("p");
  paragraph.textContent = content;
  article.append(label, paragraph);
  if (detail) {
    const small = document.createElement("small");
    small.textContent = detail;
    article.append(small);
  }
  elements.transcript.append(article);
  elements.transcript.scrollTop = elements.transcript.scrollHeight;
  return { article, paragraph };
}

async function requestJson(url, options = {}) {
  const response = await fetch(url, {
    credentials: "same-origin",
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers || {}),
    },
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(payload.error || `Request failed with ${response.status}.`);
  return payload;
}

async function typesetVector(tex) {
  if (!typesetRequests.has(tex)) {
    typesetRequests.set(
      tex,
      requestJson("/api/typeset", {
        method: "POST",
        body: JSON.stringify({ tex }),
      }).catch((error) => {
        typesetRequests.delete(tex);
        throw error;
      }),
    );
  }
  return typesetRequests.get(tex);
}

async function materializeScene(scene) {
  const nodes = [];
  for (const node of scene.nodes) {
    if (node.type !== "mathTex") {
      nodes.push(node);
      continue;
    }
    elements.renderStatus.textContent = "Typesetting LaTeX to vector paths…";
    const result = await typesetVector(node.tex);
    const { tex: _tex, fontSize, ...base } = node;
    nodes.push({
      ...base,
      type: "svg",
      svg: result.svg,
      height: fontSize,
      preserveStyles: false,
    });
  }
  return validateScene({ ...scene, nodes });
}

function renderSceneControls(scene) {
  elements.sceneControlPanel.hidden = false;
  elements.timelineControl.min = "0";
  elements.timelineControl.max = String(scene.duration);
  elements.timelineControl.step = "any";
  elements.timelineControl.value = "0";
  elements.timelineOutput.textContent = `0.00 / ${scene.duration.toFixed(2)} s`;
  elements.sceneControls.replaceChildren();
  timelineSceneControl = null;
  elements.sceneControls.hidden = scene.controls.length === 0;
  for (const control of scene.controls) {
    const wrapper = document.createElement("label");
    wrapper.className = "scene-control";
    const heading = document.createElement("span");
    heading.textContent = control.label;
    const output = document.createElement("output");
    output.textContent = String(control.default);
    const input = document.createElement("input");
    input.type = "range";
    input.min = String(control.min);
    input.max = String(control.max);
    input.step = String(control.step);
    input.value = String(control.default);
    input.addEventListener("input", () => {
      const value = Number(input.value);
      set_signal(control.signal, value);
      const decimals = Math.max(0, String(control.step).split(".")[1]?.length || 0);
      output.textContent = value.toFixed(decimals);
      if (control.timeline) {
        paused = true;
        set_paused(true);
        elements.pauseButton.textContent = "Resume preview";
        elements.renderStatus.textContent = "ValueTracker control at an exact compiled state";
      } else {
        elements.renderStatus.textContent = "Live control active";
      }
    });
    wrapper.append(heading, input, output);
    elements.sceneControls.append(wrapper);
    if (control.timeline) {
      timelineSceneControl = {
        control,
        input,
        output,
        signal: scene.signals.find((signal) => signal.id === control.signal),
      };
    } else {
      set_signal(control.signal, control.default);
    }
  }
}

function sampleTimelineSignal(signal, time) {
  if (!signal || time <= signal.keyframes[0].at) return signal?.keyframes[0].value ?? 0;
  for (let index = 1; index < signal.keyframes.length; index += 1) {
    const right = signal.keyframes[index];
    if (time > right.at) continue;
    const left = signal.keyframes[index - 1];
    const amount = (time - left.at) / Math.max(Number.EPSILON, right.at - left.at);
    return left.value + (right.value - left.value) * amount;
  }
  return signal.keyframes.at(-1).value;
}

function showTimelineTime(time) {
  const duration = activeScene?.duration || 0;
  const clamped = Math.min(duration, Math.max(0, time));
  elements.timelineControl.value = String(clamped);
  elements.timelineOutput.textContent = `${clamped.toFixed(2)} / ${duration.toFixed(2)} s`;
}

function syncTimeline() {
  if (wasmReady && activeScene && !timelineScrubbing) {
    let time;
    try {
      time = current_scene_time();
    } catch {
      requestAnimationFrame(syncTimeline);
      return;
    }
    showTimelineTime(time);
    if (timelineSceneControl) {
      const { control, input, output, signal } = timelineSceneControl;
      const value = sampleTimelineSignal(signal, time);
      input.value = String(value);
      const decimals = Math.max(0, String(control.step).split(".")[1]?.length || 0);
      output.textContent = value.toFixed(decimals);
    }
  }
  requestAnimationFrame(syncTimeline);
}

async function checkSession() {
  try {
    const status = await requestJson("/api/session");
    setConnected(Boolean(status.connected));
  } catch {
    setConnected(false);
  }
}

async function waitForRenderer(timeoutMs = 10000) {
  const status = byId("renderer-status");
  if (status.textContent.includes("engine running")) return;
  await new Promise((resolve, reject) => {
    const startedAt = performance.now();
    const check = () => {
      if (status.textContent.includes("engine running")) {
        resolve();
        return;
      }
      if (status.textContent.includes("unavailable")) {
        reject(new Error(byId("error-detail").textContent || "WebGPU is unavailable."));
        return;
      }
      if (performance.now() - startedAt >= timeoutMs) {
        reject(new Error("WebGPU took too long to initialize."));
        return;
      }
      setTimeout(check, 25);
    };
    check();
  });
}

async function connectKey(event) {
  event.preventDefault();
  const apiKey = elements.apiKey.value.trim();
  if (!apiKey) return;
  elements.connectButton.disabled = true;
  elements.keyStatus.textContent = "Checking with Terra…";
  showCodeError("");
  try {
    await requestJson("/api/session", {
      method: "POST",
      body: JSON.stringify({ apiKey }),
    });
    elements.apiKey.value = "";
    setConnected(true);
    addMessage("assistant", "Connected. What should we animate?");
    elements.prompt.focus();
  } catch (error) {
    setConnected(false);
    elements.keyStatus.textContent = error.message;
    elements.keyStatus.classList.add("form-status--error");
  } finally {
    elements.connectButton.disabled = false;
  }
}

async function disconnectKey() {
  try {
    await requestJson("/api/session", { method: "DELETE" });
  } finally {
    setConnected(false);
    addMessage("assistant", "OpenRouter key disconnected.");
  }
}

function bufferToBase64(buffer) {
  const bytesValue = new Uint8Array(buffer);
  let binary = "";
  for (let offset = 0; offset < bytesValue.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytesValue.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

async function uploadAssets() {
  const files = [...elements.assetInput.files];
  if (files.length === 0) return;
  elements.assetInput.disabled = true;
  const uploaded = [];
  try {
    for (const [index, file] of files.entries()) {
      elements.assetStatus.textContent = `Uploading ${index + 1}/${files.length}: ${file.name}`;
      const result = await requestJson("/api/assets", {
        method: "POST",
        body: JSON.stringify({
          name: file.name,
          data: bufferToBase64(await file.arrayBuffer()),
        }),
      });
      uploaded.push(result.name);
    }
    elements.assetStatus.textContent = `${uploaded.length} attached: ${uploaded.join(", ")}`;
  } catch (error) {
    elements.assetStatus.textContent = error.message;
    showCodeError(error.message);
  } finally {
    elements.assetInput.value = "";
    elements.assetInput.disabled = !connected || generating;
  }
}

async function generateAnimation(event) {
  event.preventDefault();
  const prompt = elements.prompt.value.trim();
  if (!prompt || generating) return;
  if (!connected) {
    elements.apiKey.focus();
    return;
  }

  addMessage("user", prompt);
  const assistantMessage = addMessage("assistant", "Writing the scene…");
  elements.prompt.value = "";
  renderCode("");
  currentScene = null;
  showCodeError("");
  elements.codeStatus.textContent = "Streaming from GPT-5.6 Terra";
  setGenerating(true);
  const requestStarted = performance.now();

  try {
    const response = await fetch("/api/animate", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ prompt }),
    });
    if (!response.ok) {
      const payload = await response.json().catch(() => ({}));
      if (response.status === 401) setConnected(false);
      throw new Error(payload.error || `Generation failed with ${response.status}.`);
    }
    if (!response.body) throw new Error("Streaming response is unavailable.");

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let pending = "";
    let sceneResult;

    while (true) {
      const { value, done } = await reader.read();
      pending += decoder.decode(value || new Uint8Array(), { stream: !done });
      const lines = pending.split("\n");
      pending = lines.pop() || "";
      for (const line of lines) {
        if (!line.trim()) continue;
        const eventData = JSON.parse(line);
        if (eventData.type === "delta") {
          renderCode(currentCode + eventData.delta);
        } else if (eventData.type === "reset") {
          renderCode("");
          elements.codeStatus.textContent = eventData.reason;
        } else if (eventData.type === "error") {
          throw new Error(eventData.message);
        } else if (eventData.type === "done") {
          sceneResult = eventData;
          renderCode(eventData.code);
          currentScene = eventData.scene;
          elements.codeShell.scrollTop = 0;
        }
      }
      if (done) break;
    }

    if (!sceneResult) throw new Error("Terra finished without a runnable scene.");
    latestModelMs = sceneResult.modelMs || performance.now() - requestStarted;
    latestCompileMs = sceneResult.compileMs || 0;
    elements.modelTime.textContent = seconds(latestModelMs);
    elements.compileTime.textContent = sceneResult.compileCached
      ? `Cached (${seconds(sceneResult.originalCompileMs || 0)} saved)`
      : seconds(latestCompileMs);
    elements.codeStatus.textContent = sceneResult.compileCached
      ? "Rust scene restored from compile cache"
      : sceneResult.repaired
        ? "Manim compiled after one repair"
        : "Manim compiled to Rust";
    assistantMessage.paragraph.textContent = `Built “${sceneResult.scene.title}” with Manim, then compiled ${sceneResult.scene.nodes.length} vector objects into Rust tracks. Rendering the video now…`;
    await runScene(sceneResult.scene, {
      record: true,
      modelMs: latestModelMs,
      compileMs: latestCompileMs,
    });
    assistantMessage.paragraph.textContent = `Finished “${sceneResult.scene.title}.”`;
    const detail = document.createElement("small");
    const compileDetail = sceneResult.compileCached
      ? `cached compile (${seconds(sceneResult.originalCompileMs || 0)} saved)`
      : `${seconds(latestCompileMs)} compile`;
    detail.textContent = `${sceneResult.scene.nodes.length} vectors · ${seconds(latestModelMs)} agent · ${compileDetail} · ${seconds(sceneResult.scene.duration * 1000)} video`;
    assistantMessage.article.append(detail);
  } catch (error) {
    showCodeError(error.message);
    elements.codeStatus.textContent = "Generation failed";
    assistantMessage.paragraph.textContent = error.message;
  } finally {
    setGenerating(false);
  }
}

function supportedVideoType() {
  const candidates = [
    "video/webm;codecs=vp9",
    "video/webm;codecs=vp8",
    "video/webm",
  ];
  return candidates.find((type) => MediaRecorder.isTypeSupported(type)) || "";
}

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function base64Bytes(value) {
  const binary = atob(value);
  const output = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) output[index] = binary.charCodeAt(index);
  return output;
}

async function mixSceneAudio(scene) {
  if (scene.audio.length === 0) return null;
  if (typeof OfflineAudioContext === "undefined") {
    throw new Error("This browser cannot decode the scene audio.");
  }
  const sampleRate = 48_000;
  const frameCount = Math.max(1, Math.ceil(scene.duration * sampleRate));
  const context = new OfflineAudioContext(2, frameCount, sampleRate);
  for (const clip of scene.audio) {
    const encoded = base64Bytes(clip.data);
    const audioBuffer = await context.decodeAudioData(
      encoded.buffer.slice(encoded.byteOffset, encoded.byteOffset + encoded.byteLength),
    );
    const source = context.createBufferSource();
    source.buffer = audioBuffer;
    const gain = context.createGain();
    gain.gain.value = 10 ** (clip.gainDb / 20);
    source.connect(gain).connect(context.destination);
    source.start(clip.startTime);
  }
  return context.startRendering();
}

async function deterministicAudioConfig(audioBuffer) {
  if (!audioBuffer) return null;
  if (typeof AudioEncoder === "undefined" || typeof AudioData === "undefined") {
    throw new Error("This browser cannot encode the scene audio.");
  }
  const config = {
    codec: "opus",
    sampleRate: audioBuffer.sampleRate,
    numberOfChannels: audioBuffer.numberOfChannels,
    bitrate: 128_000,
  };
  const support = await AudioEncoder.isConfigSupported(config).catch(() => null);
  if (!support?.supported) throw new Error("This browser cannot encode deterministic Opus audio.");
  return support.config || config;
}

async function encodeAudio(audioBuffer, config, muxer) {
  if (!audioBuffer || !config) return;
  let encoderError;
  const encoder = new AudioEncoder({
    output: (chunk, metadata) => muxer.addAudioChunk(chunk, metadata),
    error: (error) => {
      encoderError = error;
    },
  });
  encoder.configure(config);
  const packetFrames = 960;
  try {
    for (let offset = 0; offset < audioBuffer.length; offset += packetFrames) {
      if (encoder.encodeQueueSize > 8) {
        await new Promise((resolve) =>
          encoder.addEventListener("dequeue", resolve, { once: true }),
        );
      }
      const samples = new Float32Array(packetFrames * audioBuffer.numberOfChannels);
      const available = Math.min(packetFrames, audioBuffer.length - offset);
      for (let channel = 0; channel < audioBuffer.numberOfChannels; channel += 1) {
        samples.set(
          audioBuffer.getChannelData(channel).subarray(offset, offset + available),
          channel * packetFrames,
        );
      }
      const data = new AudioData({
        format: "f32-planar",
        sampleRate: audioBuffer.sampleRate,
        numberOfFrames: packetFrames,
        numberOfChannels: audioBuffer.numberOfChannels,
        timestamp: Math.round((offset * 1_000_000) / audioBuffer.sampleRate),
        data: samples,
      });
      encoder.encode(data);
      data.close();
      if (encoderError) throw encoderError;
    }
    await encoder.flush();
    if (encoderError) throw encoderError;
  } finally {
    encoder.close();
  }
}

function vttTimestamp(secondsValue) {
  const milliseconds = Math.max(0, Math.round(secondsValue * 1000));
  const hours = Math.floor(milliseconds / 3_600_000);
  const minutes = Math.floor((milliseconds % 3_600_000) / 60_000);
  const seconds = Math.floor((milliseconds % 60_000) / 1000);
  const fraction = milliseconds % 1000;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(fraction).padStart(3, "0")}`;
}

function attachCaptions(scene, filename) {
  if (currentCaptionUrl) URL.revokeObjectURL(currentCaptionUrl);
  currentCaptionUrl = undefined;
  elements.outputVideo.querySelectorAll("track").forEach((track) => track.remove());
  elements.captionDownloadLink.hidden = true;
  if (scene.captions.length === 0) return;
  const cues = scene.captions
    .map(
      (caption, index) =>
        `${index + 1}\n${vttTimestamp(caption.start)} --> ${vttTimestamp(caption.end)}\n${caption.text.replace(/\r?\n/g, "\n")}`,
    )
    .join("\n\n");
  currentCaptionUrl = URL.createObjectURL(new Blob([`WEBVTT\n\n${cues}\n`], { type: "text/vtt" }));
  const track = document.createElement("track");
  track.kind = "subtitles";
  track.label = "Manim subcaptions";
  track.srclang = "en";
  track.src = currentCaptionUrl;
  track.default = true;
  elements.outputVideo.append(track);
  elements.captionDownloadLink.href = currentCaptionUrl;
  elements.captionDownloadLink.download = `${filename}.vtt`;
  elements.captionDownloadLink.hidden = false;
}

async function deterministicVideoConfig(width, height, fps) {
  if (typeof VideoEncoder === "undefined" || typeof VideoFrame === "undefined") return null;
  const candidates = [
    { encoderCodec: "vp09.00.10.08", muxerCodec: "V_VP9" },
    { encoderCodec: "vp8", muxerCodec: "V_VP8" },
  ];
  for (const candidate of candidates) {
    const config = {
      codec: candidate.encoderCodec,
      width,
      height,
      framerate: fps,
      bitrate: 7_000_000,
      latencyMode: "quality",
    };
    const support = await VideoEncoder.isConfigSupported(config).catch(() => null);
    if (support?.supported) return { ...candidate, config: support.config || config };
  }
  return null;
}

export async function recordCanvasDeterministically(scene) {
  await nextAnimationFrame();
  const width = elements.viewport.width;
  const height = elements.viewport.height;
  const videoConfig = await deterministicVideoConfig(width, height, scene.fps);
  if (!videoConfig) return null;
  const audioBuffer = await mixSceneAudio(scene);
  const audioConfig = await deterministicAudioConfig(audioBuffer);

  const target = new ArrayBufferTarget();
  const muxer = new Muxer({
    target,
    video: {
      codec: videoConfig.muxerCodec,
      width,
      height,
      frameRate: scene.fps,
    },
    ...(audioConfig
      ? {
          audio: {
            codec: "A_OPUS",
            numberOfChannels: audioConfig.numberOfChannels,
            sampleRate: audioConfig.sampleRate,
          },
        }
      : {}),
    fastStart: "in-memory",
  });
  let encoderError;
  const encoder = new VideoEncoder({
    output: (chunk, metadata) => muxer.addVideoChunk(chunk, metadata),
    error: (error) => {
      encoderError = error;
    },
  });
  encoder.configure(videoConfig.config);

  const frameDurationUs = 1_000_000 / scene.fps;
  const frameCount = Math.max(1, Math.ceil(scene.duration * scene.fps));
  try {
    for (let index = 0; index < frameCount; index += 1) {
      seek_scene(index / scene.fps);
      await nextAnimationFrame();
      if (encoderError) throw encoderError;
      if (encoder.encodeQueueSize > 8) {
        await new Promise((resolve) =>
          encoder.addEventListener("dequeue", resolve, { once: true }),
        );
      }
      const frame = new VideoFrame(elements.viewport, {
        timestamp: Math.round(index * frameDurationUs),
        duration: Math.round(frameDurationUs),
      });
      encoder.encode(frame, {
        keyFrame: index % Math.max(1, Math.round(scene.fps * 2)) === 0,
      });
      frame.close();
    }
    await encoder.flush();
    if (encoderError) throw encoderError;
    await encodeAudio(audioBuffer, audioConfig, muxer);
    muxer.finalize();
    return {
      blob: new Blob([target.buffer], { type: "video/webm" }),
      deterministic: true,
      frameCount,
    };
  } finally {
    encoder.close();
    resume_scene();
  }
}

async function recordCanvasWithMediaRecorder(scene) {
  if (!elements.viewport.captureStream || typeof MediaRecorder === "undefined") {
    throw new Error("This browser cannot record the WebGPU canvas.");
  }
  const stream = elements.viewport.captureStream(scene.fps);
  const mimeType = supportedVideoType();
  const chunks = [];
  const recorder = new MediaRecorder(stream, {
    ...(mimeType ? { mimeType } : {}),
    videoBitsPerSecond: 7_000_000,
  });
  const stopped = new Promise((resolve, reject) => {
    recorder.addEventListener("stop", resolve, { once: true });
    recorder.addEventListener("error", () => reject(new Error("Canvas recording failed.")), {
      once: true,
    });
  });
  recorder.addEventListener("dataavailable", (event) => {
    if (event.data.size > 0) chunks.push(event.data);
  });

  recorder.start(100);
  await new Promise((resolve) => requestAnimationFrame(() => resolve()));
  await new Promise((resolve) => setTimeout(resolve, scene.duration * 1000));
  recorder.stop();
  await stopped;
  stream.getTracks().forEach((track) => track.stop());
  const blob = new Blob(chunks, { type: recorder.mimeType || "video/webm" });
  if (blob.size === 0) throw new Error("The browser produced an empty video.");
  return {
    blob,
    deterministic: false,
    frameCount: Math.ceil(scene.duration * scene.fps),
  };
}

async function recordCanvas(scene) {
  set_render_size(scene.pixelWidth, scene.pixelHeight);
  await nextAnimationFrame();
  try {
    const deterministic = await recordCanvasDeterministically(scene);
    return deterministic || recordCanvasWithMediaRecorder(scene);
  } finally {
    clear_render_size();
    await nextAnimationFrame();
  }
}

export async function runScene(sceneCandidate, options = {}) {
  const sourceScene = validateScene(sceneCandidate);
  if (!wasmReady) throw new Error("The Rust renderer is still starting.");
  showCodeError("");
  elements.sceneTitle.textContent = sourceScene.title;
  elements.renderStatus.textContent = options.record ? "Rendering output video…" : "Running live preview";
  elements.runButton.disabled = true;
  elements.pauseButton.disabled = true;
  if (options.record) elements.videoOutput.hidden = true;
  paused = false;
  set_paused(false);
  const renderStarted = performance.now();

  try {
    const scene = await materializeScene(sourceScene);
    activeScene = scene;
    load_scene(JSON.stringify(scene));
    renderSceneControls(scene);
    if (options.record) {
      const recording = await recordCanvas(scene);
      const { blob } = recording;
      const renderMs = performance.now() - renderStarted;
      if (currentVideoUrl) URL.revokeObjectURL(currentVideoUrl);
      currentVideoUrl = URL.createObjectURL(blob);
      elements.outputVideo.src = currentVideoUrl;
      elements.downloadLink.href = currentVideoUrl;
      const filename = scene.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "animation";
      elements.downloadLink.download = `${filename}.webm`;
      attachCaptions(scene, filename);
      const audioDescription = scene.audio.length ? ` · ${scene.audio.length} audio clip${scene.audio.length === 1 ? "" : "s"}` : "";
      const captionDescription = scene.captions.length ? ` · ${scene.captions.length} caption${scene.captions.length === 1 ? "" : "s"}` : "";
      elements.videoDescription.textContent = `${scene.duration.toFixed(1)} seconds · ${scene.fps} fps · ${recording.frameCount} exact frames · WebM${audioDescription}${captionDescription}${recording.deterministic ? "" : " (real-time fallback)"}`;
      elements.videoSize.textContent = bytes(blob.size);
      elements.renderTime.textContent = seconds(renderMs);
      elements.totalTime.textContent = seconds(
        (options.modelMs || 0) + (options.compileMs || 0) + renderMs,
      );
      elements.videoOutput.hidden = false;
      elements.renderStatus.textContent = "Video ready";
      await elements.outputVideo.play().catch(() => {});
    } else {
      reset_clock();
      elements.renderTime.textContent = "Live";
      elements.totalTime.textContent = "—";
      elements.renderStatus.textContent = "Running live preview";
    }
  } finally {
    elements.runButton.disabled = generating;
    elements.pauseButton.disabled = false;
    elements.pauseButton.textContent = "Pause preview";
  }
}

async function runCurrentCode() {
  try {
    const scene = currentScene || parseSceneCode(currentCode);
    latestModelMs = 0;
    latestCompileMs = 0;
    elements.modelTime.textContent = "Manual";
    elements.compileTime.textContent = currentScene ? "Cached" : "Manual";
    await runScene(scene, { record: true, modelMs: 0, compileMs: 0 });
  } catch (error) {
    showCodeError(error.message);
  }
}

async function copyCode() {
  await navigator.clipboard.writeText(currentCode);
  elements.copyButton.textContent = "Copied";
  setTimeout(() => {
    elements.copyButton.textContent = "Copy";
  }, 1200);
}

function togglePause() {
  paused = !paused;
  if (paused) {
    set_paused(true);
  } else {
    timelineScrubbing = false;
    resume_scene();
  }
  elements.pauseButton.textContent = paused ? "Resume preview" : "Pause preview";
  elements.renderStatus.textContent = paused ? "Preview paused" : "Running live preview";
}

function scrubTimeline() {
  timelineScrubbing = true;
  paused = true;
  set_paused(true);
  const time = Number(elements.timelineControl.value);
  seek_scene(time);
  showTimelineTime(time);
  elements.pauseButton.textContent = "Resume preview";
  elements.renderStatus.textContent = "Preview scrubbed to an exact frame";
}

function finishTimelineScrub() {
  timelineScrubbing = false;
  showTimelineTime(Number(elements.timelineControl.value));
}

elements.keyForm.addEventListener("submit", connectKey);
elements.disconnectButton.addEventListener("click", disconnectKey);
elements.assetInput.addEventListener("change", uploadAssets);
elements.promptForm.addEventListener("submit", generateAnimation);
elements.runButton.addEventListener("click", runCurrentCode);
elements.copyButton.addEventListener("click", copyCode);
elements.pauseButton.addEventListener("click", togglePause);
elements.timelineControl.addEventListener("input", scrubTimeline);
elements.timelineControl.addEventListener("change", finishTimelineScrub);
elements.prompt.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
    elements.promptForm.requestSubmit();
  }
});

renderCode(currentCode);
elements.codeShell.scrollTop = 0;
elements.videoOutput.hidden = true;
elements.outputVideo.removeAttribute("src");
setConnected(false);
await checkSession();

try {
  await init();
  webPlayer = await create_player(elements.viewport);
  byId("renderer-status").textContent = "General WebGPU engine running";
  byId("backend-value").textContent = "Rust scene IR → lyon → WebGPU";
  await waitForRenderer();
  wasmReady = true;
  requestAnimationFrame(syncTimeline);
  elements.runButton.disabled = false;
  byId("gpu-dot").classList.remove("runtime-dot--error");
  byId("error-state").className = "error-state";
  await runScene(DEFAULT_SCENE, { record: false });
} catch (error) {
  const status = byId("renderer-status");
  status.textContent = "WebGPU unavailable";
  byId("gpu-dot").classList.add("runtime-dot--error");
  byId("error-detail").textContent = error instanceof Error ? error.message : String(error);
  byId("error-state").className = "error-state error-state--visible";
  showCodeError(error instanceof Error ? error.message : String(error));
  console.error(error);
}
