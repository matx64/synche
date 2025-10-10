const es = new EventSource("/api/events");

es.onmessage = (event) => {
  console.log("New message from server:", event);
};

async function send_msg() {
  await fetch("/api/send");
}
