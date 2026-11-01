"""The "try it in a sandbox" service behind usevintage.site/try/, on Beam (beam.cloud).

- IMAGE: Ubuntu 24.04 with `vintage` built from this public repository at COMMIT, the signed event
  registry and demo/ in /workspace, ttyd (a terminal in the browser), and a warm root cache so the
  first checks are quick.
- launcher: a public HTTP endpoint. POST starts one sandbox from IMAGE: 2 vCPU, 1 GiB, deleted after
  LIFETIME seconds, outbound traffic only to the Zcash data servers and our API, ttyd on PORT. It
  answers {"url", "expires_at", "id", "key"} or {"error", "message"}. POST /end with {"id", "key"} (as
  text/plain, so browsers send no preflight and sendBeacon works) deletes that sandbox at once: the page's
  End button, and leaving the page. Only the launch's one-time key can end it; the launcher keeps a hash.

Lifetime: Beam's keep_warm_seconds is an idle window (an open terminal connection keeps a sandbox alive),
so it can't be the 15-minute limit by itself. The limit is enforced three ways: a timer inside the sandbox
closes the terminal at LIFETIME; the idle window is IDLE, so a sandbox whose page closed dies soon after; and
every launch first deletes any sandbox past its LIFETIME.

Limits: at most MAX_LIVE sandboxes at once, MAX_PER_DAY launches per UTC day, MAX_PER_MONTH per
month. The only state is those counts, the live sandboxes' expiry times, and each live sandbox's key hash
(a Beam Map). No IP, no request log, nothing about the visitor.

Deploy from a folder that holds only this file (beam deploy uploads the working directory):
  beam deploy vintage_try.py:launcher
"""

import base64
import hashlib
import json
import secrets
import time
import urllib.request

from beam import Image, Map, Sandbox, asgi

COMMIT = "8b08738"
PORT = 7681
LIFETIME = 900
IDLE = 300
MAX_LIVE, MAX_PER_DAY, MAX_PER_MONTH = 3, 40, 300
ALLOW = ["zec.rocks", "testnet.zec.rocks", "api.usevintage.site"]

TX_STRANGER = "d3252057850259c86ffae437d0df207ff8b500e4d83dfe4409f585dcd6b441e4"
WELCOME = """
  VINTAGE sandbox: proof of age for private money.
  vintage, the signed event registry and demo/ are here.
  Paste a command from the page, or try: vintage --help
  This machine reaches only zec.rocks and our API.
  It is deleted after 15 minutes. Nothing you type is kept.

"""

IMAGE = Image(
    base_image="docker.io/library/ubuntu:24.04",
    commands=[
        "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "
        "ca-certificates curl git build-essential pkg-config ttyd jq && rm -rf /var/lib/apt/lists/*",
        "curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable",
        f"git clone https://github.com/Samuel-Chuku/vintage /opt/vintage && cd /opt/vintage && git checkout {COMMIT}",
        "cd /opt/vintage && $HOME/.cargo/bin/cargo build --release --locked -p vintage-cli "
        "&& install -m 755 target/release/vintage /usr/local/bin/vintage "
        "&& rm -rf target $HOME/.cargo/registry && $HOME/.cargo/bin/rustup self uninstall -y",
        "mkdir -p /workspace && cp -r /opt/vintage/registry /opt/vintage/demo /workspace/",
        # Warm the root cache: later checks only fetch blocks newer than the image.
        f"cd /workspace && vintage verify {TX_STRANGER} --before bitget-2026-09 > /dev/null; "
        f"vintage age {TX_STRANGER} > /dev/null; "
        "vintage link verify demo/mainnet-zk-link.json --before vintage-demo-2026-10 > /dev/null; "
        "vintage --network test link verify demo/testnet-zk-link.json --before 4562500 > /dev/null; true",
        f"echo {base64.b64encode(WELCOME.encode()).decode()} | base64 -d > /etc/vintage-welcome",
        "printf 'cat /etc/vintage-welcome\\ncd /workspace\\nPS1=\"vintage:\\\\w$ \"\\n' > /etc/vintage-bashrc",
    ],
)


def start_sandbox():
    """One sandbox with the terminal running. Returns (url, expires_at, sandbox id)."""
    sb = Sandbox(
        name="vintage-try",
        image=IMAGE,
        cpu=2,
        memory="1Gi",
        keep_warm_seconds=IDLE,
        ports=[PORT],
        allow_list=ALLOW,
        env={"HOME": "/root"},  # the image warmed /root/.cache/vintage
    ).create()
    expires_at = int(time.time()) + LIFETIME
    sb.process.exec(
        "ttyd", "-W", "-p", str(PORT), "-m", "2", "-t", "titleFixed=vintage", "-t", "fontSize=14",
        "bash", "--rcfile", "/etc/vintage-bashrc", cwd="/workspace",
    )
    # Close the terminal at LIFETIME whatever the browser does; with nothing connected the sandbox then goes
    # idle and Beam deletes it IDLE seconds later.
    sb.process.exec("sh", "-c", f"sleep {LIFETIME}; pkill -x ttyd; pkill -x bash; true")
    url = sb.list_urls().get(PORT) or sb.expose_port(PORT)
    # Hand the URL over only once the terminal answers.
    for _ in range(40):
        try:
            if urllib.request.urlopen(url, timeout=3).status == 200:
                break
        except Exception:
            time.sleep(0.5)
    return url, expires_at, sb.container_id


def free_slot(state: Map, expires_at: int):
    """Remove one live sandbox's expiry, so its slot opens now instead of at expiry."""
    live = list(state.get("live") or [])
    if expires_at in live:
        live.remove(expires_at)
        state.set("live", live, ttl=86_400)


def sweep(state: Map, now: int):
    """Delete every sandbox past its LIFETIME (a backstop for the in-sandbox timer and the idle window)."""
    keep = []
    for sid, slot in state.get("open") or []:
        if slot > now:
            keep.append((sid, slot))
            continue
        try:
            Sandbox().connect(sid).terminate()
        except Exception:
            pass  # already gone
    state.set("open", keep, ttl=86_400)


def admit(state: Map, now: int):
    """Check the limits and record one launch. Returns an error dict, or None if admitted."""
    live = [t for t in (state.get("live") or []) if t > now]
    day, month = time.strftime("%Y-%m-%d", time.gmtime(now)), time.strftime("%Y-%m", time.gmtime(now))
    if len(live) >= MAX_LIVE:
        return {"error": "busy", "message": f"All {MAX_LIVE} sandboxes are in use. Try again in a few minutes, "
                "or run the commands on your own machine."}
    if (state.get("day:" + day) or 0) >= MAX_PER_DAY or (state.get("month:" + month) or 0) >= MAX_PER_MONTH:
        return {"error": "limit", "message": "Today's sandboxes have all been used. Try again tomorrow, "
                "or run the commands on your own machine."}
    state.set("live", live + [now + LIFETIME], ttl=86_400)
    state.set("day:" + day, (state.get("day:" + day) or 0) + 1, ttl=2 * 86_400)
    # Map entries live at most a week; each launch rewrites (and so renews) the month's count, which only
    # lapses after a week with no launches at all, when the cap doesn't matter.
    state.set("month:" + month, (state.get("month:" + month) or 0) + 1, ttl=7 * 86_400)
    return None


@asgi(name="vintage-try", image=Image(python_packages=["fastapi"]), cpu=0.25, memory="512Mi", keep_warm_seconds=60, authorized=False)
def launcher():
    from fastapi import FastAPI, Request

    # No CORS middleware: Beam's gateway already answers with Access-Control-Allow-Origin: *, and a
    # second header would make browsers reject the response. The page sends no cookies or credentials.
    app = FastAPI(docs_url=None, redoc_url=None, openapi_url=None)
    state = Map(name="vintage-try-limits")

    @app.post("/")
    def launch():
        now = int(time.time())
        sweep(state, now)
        refused = admit(state, now)
        if refused:
            return refused
        try:
            url, expires_at, sid = start_sandbox()
        except Exception:
            print("launch: sandbox error")  # class only: nothing about the request
            free_slot(state, now + LIFETIME)  # no sandbox is running for it
            return {"error": "failed", "message": "Couldn't start a sandbox just now. Try again in a minute."}
        state.set("open", list(state.get("open") or []) + [(sid, now + LIFETIME)], ttl=86_400)
        key = secrets.token_urlsafe(24)
        state.set("end:" + sid, {"h": hashlib.sha256(key.encode()).hexdigest(), "slot": now + LIFETIME}, ttl=LIFETIME + 300)
        return {"url": url, "expires_at": expires_at, "id": sid, "key": key}

    @app.post("/end")
    async def end(request: Request):
        try:
            body = json.loads(await request.body())
            sid, key = str(body["id"]), str(body["key"])
        except Exception:
            return {"error": "bad request"}
        rec = state.get("end:" + sid)
        if not rec or not secrets.compare_digest(rec["h"], hashlib.sha256(key.encode()).hexdigest()):
            return {"error": "unknown sandbox"}
        try:
            Sandbox().connect(sid).terminate()
        except Exception:
            print("end: terminate error")  # class only
        del state["end:" + sid]
        free_slot(state, rec["slot"])
        state.set("open", [o for o in (state.get("open") or []) if o[0] != sid], ttl=86_400)
        return {"ended": True}

    return app
