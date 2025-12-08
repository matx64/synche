import {
  addPeerToList,
  setPeerAsDisconnected,
  addDirToList,
  removeDirFromList
} from './components.js';

const el_peer_list = document.getElementById("peer-list");
const el_dir_list = document.getElementById("dir-list");

const es = new EventSource("/api/events");

es.onmessage = (event) => {
  const data = JSON.parse(event.data);

  console.log("SSE:", data);

  if (typeof data === "string") {
    if (data === "ServerRestart") {
      reload();
    }
    return;
  }

  const [kind, payload] = Object.entries(data)[0];

  switch (kind) {
    case "PeerConnected":
      addPeerToList(payload, el_peer_list);
      break;

    case "PeerDisconnected":
      setPeerAsDisconnected(payload);
      break;

    case "SyncDirectoryAdded":
      addDirToList(payload, el_dir_list);
      break;

    case "SyncDirectoryRemoved":
      removeDirFromList(payload);
      break;
  }
};

es.onerror = (error) => {
  console.error("SSE connection error:", error);
  reload();
};

function reload() {
  console.log("Server is restarting or connection error. Reloading page...");
  es.close();
  setTimeout(() => {
    window.location.reload();
  }, 1000);
}
