// The built-in Remote Control page: a small proof of the API (docs/remote-api.md). It keeps
// one WebSocket, resumes from the version it last had, shows what is on screen with the live
// preview and the playlist in front, and sends Next, Previous and Clear All as the same
// commands `sp` sends. Every word from the app is put in with textContent, never as HTML.
"use strict";
(function () {
  const $ = (id) => document.getElementById(id);
  const state = { epoch: null, version: 0, parts: {} };
  let socket = null;
  let nextId = 1;
  const waiting = new Map();
  let retry = 500;
  let previewUrl = null;

  function setConnection(words, tone) {
    const pill = $("connection");
    pill.textContent = words;
    pill.className = "pill " + (tone || "");
  }

  function connect() {
    setConnection("Connecting…");
    socket = new WebSocket("wss://" + location.host + "/api/ws");
    socket.binaryType = "blob";
    socket.addEventListener("open", () => {
      retry = 500;
      const hello = { type: "hello", preview: true };
      if (state.epoch) hello.resume = { epoch: state.epoch, version: state.version };
      socket.send(JSON.stringify(hello));
    });
    socket.addEventListener("message", (event) => {
      if (typeof event.data !== "string") return showPreview(event.data);
      let message;
      try { message = JSON.parse(event.data); } catch (_) { return; }
      take(message);
    });
    socket.addEventListener("close", (event) => {
      setConnection("Not connected", "off");
      for (const [, done] of waiting) done({ ok: false, text: "Not connected." });
      waiting.clear();
      // Signed out (forgotten, or a new password): back to the sign-in page.
      if (event.code === 4401) { location.replace("/login"); return; }
      // Disconnected at the computer: stays so until asked to connect again.
      if (event.code === 4000) {
        setConnection("Disconnected at the computer · tap to connect", "off");
        $("connection").addEventListener("click", connect, { once: true });
        return;
      }
      setTimeout(connect, retry);
      retry = Math.min(retry * 2, 8000);
    });
  }

  function take(message) {
    switch (message.type) {
      case "snapshot":
        state.epoch = message.epoch;
        state.version = message.version;
        state.parts = message.parts || {};
        $("you").textContent = message.you ? message.you.device + " · " + message.you.role : "";
        setConnection("Connected", "on");
        render();
        break;
      case "change":
        state.version = message.version;
        Object.assign(state.parts, message.parts || {});
        setConnection("Connected", "on");
        render();
        break;
      case "reply": {
        const done = waiting.get(message.id);
        waiting.delete(message.id);
        if (done) done(message.reply);
        break;
      }
      case "error":
        $("said").textContent = message.text || "";
        break;
    }
  }

  function showPreview(blob) {
    // The first byte says what the picture is: 1 is the live preview, a JPEG after it.
    blob.slice(0, 1).arrayBuffer().then((head) => {
      if (new Uint8Array(head)[0] !== 1) return;
      const url = URL.createObjectURL(blob.slice(1, blob.size, "image/jpeg"));
      $("preview").src = url;
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      previewUrl = url;
    });
  }

  function send(args) {
    return new Promise((resolve) => {
      if (!socket || socket.readyState !== WebSocket.OPEN) return resolve({ ok: false, text: "Not connected." });
      const id = nextId++;
      waiting.set(id, resolve);
      socket.send(JSON.stringify({ type: "command", id, args }));
    });
  }

  async function run(args) {
    const reply = await send(args);
    $("said").textContent = reply.text || (reply.ok ? "Done." : "Refused.");
  }

  function render() {
    const live = state.parts.live || {};
    const shown = live.live;
    $("live").textContent = shown
      ? (shown.song || "") + " · slide " + shown.slide + " of " + shown.slides
      : "Nothing is on screen.";
    $("words").textContent = shown && shown.words ? shown.words : "";
    const list = state.parts.playlist || {};
    $("playlist-name").textContent = list.name || "Playlist";
    const items = $("items");
    items.replaceChildren();
    for (const item of list.items || []) {
      const row = document.createElement("li");
      row.textContent = item.title;
      if (item.kind === "header") row.className = "header";
      // `item` is the place in the playlist (`number` is a song's in the library).
      if (list.live_item === item.item) row.classList.add("live");
      items.appendChild(row);
    }
  }

  $("next").addEventListener("click", () => run(["next"]));
  $("previous").addEventListener("click", () => run(["previous"]));
  $("clear").addEventListener("click", () => run(["clear", "all"]));
  connect();
})();
