// What the extension thinks is true, so "it isn't working" has an answer.
//
// Every part of this can fail on its own — the redirect rule may not be
// registered, the host access may be withheld, the bundle may not have
// been built — and each failure looks identical from a web page: nothing
// happens. This says which.

const state = document.getElementById("state");
const toggle = document.getElementById("toggle");
const advice = document.getElementById("advice");

const row = (label, value, ok) =>
  `<div class="row"><span>${label}</span><span class="${ok ? "ok" : "bad"}">${value}</span></div>`;

async function report() {
  const rules = await chrome.declarativeNetRequest.getDynamicRules();
  const redirecting = rules.some((r) => r.action?.type === "redirect");

  // Host access can be withheld even when the manifest asks for it, and
  // then the redirect fires and the fetch fails — or does not fire at all.
  const access = await chrome.permissions.contains({ origins: ["<all_urls>"] });

  // The wasm has to have been built and copied in.
  let bundle = false;
  try {
    const response = await fetch(chrome.runtime.getURL("dist/build.txt"));
    bundle = response.ok;
  } catch {
    bundle = false;
  }

  state.innerHTML =
    row("redirect rule", redirecting ? "registered" : "missing", redirecting) +
    row("site access", access ? "all sites" : "withheld", access) +
    row("viewer bundle", bundle ? "present" : "not built", bundle);

  toggle.textContent = redirecting ? "Turn the redirect off" : "Turn the redirect on";

  const notes = [];
  if (!access) {
    notes.push(
      "Chrome is withholding access to sites. On the extension's card, " +
        "set <code>Site access</code> to <code>On all sites</code>."
    );
  }
  if (!bundle) {
    notes.push("Run <code>./extension/build.sh</code>, then reload the extension.");
  }
  if (access && bundle && !redirecting) {
    notes.push("The redirect is off. Turn it on below.");
  }
  if (!notes.length) {
    notes.push(
      "Open a <code>.csv</code>, <code>.tsv</code>, <code>.json</code> or " +
        "<code>.xml</code> URL and it will show in vuwr."
    );
  }
  advice.innerHTML = notes.join("<br><br>");
}

toggle.addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ toggle: true });
  await report();
});

report().catch((e) => {
  state.textContent = `could not read the extension's own state: ${e}`;
});
