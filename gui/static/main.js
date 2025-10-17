// const es = new EventSource("/api/events");

// es.onmessage = (event) => {
//   console.log("New message from server:", event);
// };

const el_dir_form = document.getElementById("add-dir-form");
const el_dir_list = document.getElementById("dir-list");

el_dir_form.addEventListener("submit", async (e) => {
  e.preventDefault();

  const data = new FormData(el_dir_form);
  const dir_name = data.get("dir-name");

  el_dir_form.closest("dialog").close();

  const res = await fetch(`/api/add-sync-dir?name=${dir_name}`, {
    method: "POST",
  });

  if (res.status == 201) {
    el_dir_list.insertAdjacentHTML(
      "beforeend",
      `<div>📂 <strong>${dir_name}</strong> Up to Date</div>`
    );
  }
});
