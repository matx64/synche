const el_peer_list = document.getElementById("peer-list");

const es = new EventSource("/api/events");

es.onmessage = (event) => {
  const data = JSON.parse(event.data);

  console.log("SSE:", data);

  const [kind, payload] = Object.entries(data)[0];

  switch (kind) {
    case "PeerConnected":
      render_peer_list_item_component(payload);
      break;

    case "PeerDisconnected":
      disconnect_peer(payload);
      break;
  }
};

function render_peer_list_item_component({ id, addr, hostname }) {
  document.getElementById(`peer-${id}`)?.remove();

  const component = `<details class="list-item" id="peer-${id}">
            <summary><strong>🖥️ ${hostname}</strong></summary>
            <p><strong>Status: </strong> <span class="peer-status">🟢 Connected</span></p>
            <p><strong>IP:</strong> ${addr}</p>
            <p><strong>ID:</strong> ${id}</p>
          </details>`;

  el_peer_list.insertAdjacentHTML("beforeend", component);
}

function disconnect_peer(id) {
  const el = document.getElementById(`peer-${id}`);
  if (!el) return;

  const status = el.querySelector(".peer-status");
  if (status) status.innerHTML = "🔴 Disconnected";
}
