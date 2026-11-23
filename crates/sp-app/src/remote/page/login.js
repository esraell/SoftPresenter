// Signs this device in with the password; the computer answers with a cookie this page can't
// read, and the device is remembered.
"use strict";
(function () {
  const form = document.getElementById("signin");
  const field = document.getElementById("password");
  const said = document.getElementById("said");
  // The button waits for this script, so pressing Enter early can never send the password some other way.
  document.getElementById("signinGo").disabled = false;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    said.textContent = "Signing in…";
    try {
      const answer = await fetch("/api/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password: field.value }),
        credentials: "same-origin",
      });
      const reply = await answer.json().catch(() => ({}));
      if (answer.ok && reply.ok) {
        field.value = "";
        location.replace("/");
        return;
      }
      said.textContent = reply.text || "Could not sign in.";
    } catch (problem) {
      said.textContent = "The computer didn't answer.";
    }
  });
})();
