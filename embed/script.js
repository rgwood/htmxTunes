import { h, render } from "/preact.js";
import htm from "/htm.js";

const html = htm.bind(h);

let url = new URL("/events", window.location.href);
// http => ws
// https => wss
url.protocol = url.protocol.replace("http", "ws");

let events = [];

let ws = new WebSocket(url.href);
ws.onmessage = async (ev) => {
  let deserialized = JSON.parse(ev.data);
  events.push(deserialized);
  main();
};

ws.onclose = (_) => {
  events.push({ type: "Disconnected" });
  main();
};

function main() {
  render(
    html`
    <div class="text-xl text-cyan-50">Title</div>
    ${events.map((event) => html`<div>${event}</div>`)}
    `,
    document.body
  );
}

main();