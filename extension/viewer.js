// The page the redirect lands on: fetch the file, hand it to vuwr.
//
// The fetch happens here rather than in vuwr because this page runs on
// the extension's own origin and carries its host permissions, so it can
// read a file whose server says nothing about CORS — which is every feed.
// vuwr itself still makes no network request: it is given bytes.

const notice = document.getElementById("notice");
const say = (html) => {
  notice.innerHTML = html;
  notice.style.display = "grid";
};

/** The file's name, for the title bar and to choose a parser. */
function nameOf(url) {
  try {
    return decodeURIComponent(new URL(url).pathname.split("/").pop() || "file");
  } catch {
    return "file";
  }
}

async function main() {
  // The raw tail rather than `searchParams`, because the address is
  // spliced in by the redirect rule and cannot be encoded there: a feed
  // URL carrying `?utm_source=x&utm_medium=y` would otherwise arrive with
  // its own query read as ours and everything after the first `&` lost.
  const marker = "?url=";
  const at = location.href.indexOf(marker);
  const target = at === -1 ? null : location.href.slice(at + marker.length);

  // Both halves stamped with the build id, as on the hosted page: a
  // rebuild that paired a fresh module with a cached loader fails in a
  // way that reads like a broken build.
  const build = (
    await (await fetch("dist/build.txt", { cache: "no-store" })).text()
  ).trim();
  const { default: init, start, open_bytes } = await import(
    `./dist/vuwr.js?v=${build}`
  );
  await init({ module_or_path: `./dist/vuwr_bg.wasm?v=${build}` });
  await start("vuwr");

  if (!target) {
    // Opened from the toolbar rather than by following a link: vuwr's own
    // drop zone is the right thing to show.
    notice.style.display = "none";
    return;
  }

  document.title = nameOf(target);
  say(`fetching ${nameOf(target)}…`);
  try {
    const response = await fetch(target, { credentials: "include" });
    if (!response.ok) {
      say(
        `<div><strong>${response.status} ${response.statusText}</strong></div>
         <div>${escapeHtml(target)}</div>
         <div><a href="${escapeHtml(target)}">open it in the browser instead</a></div>`
      );
      return;
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    open_bytes(nameOf(target), bytes);
    notice.style.display = "none";
  } catch (e) {
    // A file the extension cannot reach, or one that is not there. Say so
    // and offer the way out rather than leaving an empty canvas.
    say(
      `<div><strong>could not read that file</strong></div>
       <div>${escapeHtml(String(e))}</div>
       <div><a href="${escapeHtml(target)}">open it in the browser instead</a></div>`
    );
  }
}

function escapeHtml(s) {
  return String(s).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );
}

main().catch((e) => {
  say(`<div><strong>vuwr failed to start</strong></div><div>${escapeHtml(String(e))}</div>`);
  console.error(e);
});
