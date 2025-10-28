const es = new EventSource("/api/events");

es.onmessage = (event) => {
  console.log("New message from server:", JSON.parse(event.data));
};
