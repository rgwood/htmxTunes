// import { h, render } from "/preact.js";
// import htm from "/htm.js";

// const html = htm.bind(h);

let url = new URL("/events", window.location.href);
// http => ws
// https => wss
url.protocol = url.protocol.replace("http", "ws");

let events = [];

let debug_mode = false;

let ws = new WebSocket(url.href);
ws.onmessage = async (ev) => {
  let deserialized = JSON.parse(ev.data);
  if (!!deserialized.debug_mode) {
    console.log("Debug mode enabled");
    debug_mode = true;
  }
};

ws.onclose = (_) => {
    console.log("Disconnected");
    if (debug_mode) {
        window.close();
    }
};
