#!/usr/bin/env python3
"""Load a generated data pack into a real Minecraft 26.3 dedicated server and report problems.

Parse-time errors (bad syntax in a .mcfunction) show up when the pack loads, and errors in
commands show up when the functions run. Use it to check that the datapack generator's output
is actually accepted by the game, not just that it matches our docs.

  tools/verify_pack.py PACK_DIR_OR_ZIP [--run ns:fn ...] [--java PATH] [--jar PATH] [--keep]

Needs Java 25+ (26.3 requirement). The server jar is downloaded from Mojang's official
endpoints once and cached in target/mc/. By running it you accept the Minecraft EULA.
"""
import argparse
import hashlib
import json
import os
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

VERSION = "26.3"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.join(ROOT, "target", "mc")
MANIFEST = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"

# Lines that mean "the game rejected our output" (as opposed to ordinary runtime failures
# such as "Found no elements matching", which are expected when running a test pack).
PARSE_ERRORS = re.compile(
    r"Failed to (load|instantiate|parse)|Whilst parsing|Unknown or incomplete command|"
    r"Incorrect argument|Expected |Invalid |Unknown (function|tag|scoreboard objective)|"
    r"Couldn't (parse|load)|Failed to read|Unable to parse"
)


def find_java(explicit):
    candidates = [explicit, os.environ.get("JAVA"), "/usr/lib/jvm/java-27-openjdk/bin/java",
                  "/usr/lib/jvm/java-25-openjdk/bin/java", shutil.which("java")]
    for c in candidates:
        if c and os.path.exists(c):
            out = subprocess.run([c, "-version"], capture_output=True, text=True).stderr
            m = re.search(r'version "(\d+)', out)
            if m and int(m.group(1)) >= 25:
                return c
    sys.exit("need Java 25 or newer for Minecraft 26.3 (pass --java PATH or set JAVA)")


def fetch_jar():
    path = os.path.join(CACHE, f"server-{VERSION}.jar")
    if os.path.exists(path):
        return path
    os.makedirs(CACHE, exist_ok=True)
    print(f"downloading Minecraft {VERSION} server from Mojang ...", file=sys.stderr)
    manifest = json.load(urllib.request.urlopen(MANIFEST))
    entry = next(v for v in manifest["versions"] if v["id"] == VERSION)
    server = json.load(urllib.request.urlopen(entry["url"]))["downloads"]["server"]
    data = urllib.request.urlopen(server["url"]).read()
    if hashlib.sha1(data).hexdigest() != server["sha1"]:
        sys.exit("downloaded server.jar failed its sha1 check")
    with open(path, "wb") as f:
        f.write(data)
    return path


class Server:
    def __init__(self, java, jar, run_dir):
        self.proc = subprocess.Popen(
            [java, "-Xmx1G", "-jar", jar, "--nogui"], cwd=run_dir, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        self.q = queue.Queue()
        self.log = []
        threading.Thread(target=self._read, daemon=True).start()
        self.n = 0

    def _read(self):
        for line in self.proc.stdout:
            self.q.put(line.rstrip("\n"))
        self.q.put(None)

    def wait_for(self, pattern, timeout):
        end = time.time() + timeout
        lines = []
        while time.time() < end:
            try:
                line = self.q.get(timeout=0.5)
            except queue.Empty:
                continue
            if line is None:
                return lines, False
            lines.append(line)
            if re.search(pattern, line):
                return lines, True
        return lines, False

    def command(self, cmd, timeout=30):
        """Runs a console command and returns every log line produced until it settled."""
        self.n += 1
        sentinel = f"__slab_done_{self.n}__"
        self.proc.stdin.write(cmd + "\n")
        self.proc.stdin.write(f"say {sentinel}\n")
        self.proc.stdin.flush()
        lines, ok = self.wait_for(re.escape(sentinel) + r"$", timeout)
        if not ok:
            lines.append(f"(timeout waiting for `{cmd}`)")
        out = [re.sub(r"^\[[^\]]*\] \[[^\]]*\]: ", "", l) for l in lines if sentinel not in l]
        return out

    def stop(self):
        try:
            self.proc.stdin.write("stop\n")
            self.proc.stdin.flush()
            self.proc.wait(30)
        except Exception:
            self.proc.kill()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pack")
    ap.add_argument("--setup", nargs="*", default=[], help="console commands to run before --run (e.g. `forceload add 0 0` so entities can be found)")
    ap.add_argument("--settle", type=float, default=30, help="after --setup, wait up to this many seconds for the chunk at 0 0 to become entity-ticking")
    ap.add_argument("--run", nargs="*", default=[], help="function ids to run after loading")
    ap.add_argument("--checks", help="JSON file: [{\"cmd\": console command, \"expect\": substring | \"matches\": regex | \"absent\": substring that must NOT appear, \"wait\": seconds (optional)}, ...] run after --run")
    ap.add_argument("--java")
    ap.add_argument("--jar")
    ap.add_argument("--keep", action="store_true", help="keep the temporary server directory")
    args = ap.parse_args()

    java = find_java(args.java)
    jar = args.jar or fetch_jar()
    pack = os.path.abspath(args.pack)
    if not os.path.exists(pack):
        sys.exit(f"no such pack: {pack}")

    run_dir = tempfile.mkdtemp(prefix="slab-verify-", dir=os.path.join(ROOT, "target") if os.path.isdir(os.path.join(ROOT, "target")) else None)
    problems = []
    try:
        with open(os.path.join(run_dir, "eula.txt"), "w") as f:
            f.write("eula=true\n")
        with open(os.path.join(run_dir, "server.properties"), "w") as f:
            f.write("online-mode=false\ngenerate-structures=false\n"
                    "view-distance=2\nsimulation-distance=2\nspawn-protection=0\n"
                    "function-permission-level=2\nenable-jmx-monitoring=false\nmax-tick-time=-1\n")
        packs_dir = os.path.join(run_dir, "world", "datapacks")
        os.makedirs(packs_dir)
        name = os.path.basename(pack.rstrip("/"))
        dest = os.path.join(packs_dir, name)
        if os.path.isdir(pack):
            shutil.copytree(pack, dest)
        else:
            shutil.copy(pack, dest)

        srv = Server(java, jar, run_dir)
        startup, ok = srv.wait_for(r"Done \(", 180)
        if not ok:
            print("\n".join(startup[-30:]))
            sys.exit("server did not start")
        for l in startup:
            if PARSE_ERRORS.search(l) or "ERROR" in l:
                problems.append(l)

        print("== datapack enable / reload ==")
        out = srv.command(f'datapack enable "file/{name}"')
        out += srv.command("reload", timeout=60)
        out += srv.command("datapack list enabled")
        for l in out:
            print(l)
            if PARSE_ERRORS.search(l) or "ERROR" in l:
                problems.append(l)

        for c in args.setup:
            print(f"== setup: {c} ==")
            for l in srv.command(c):
                print(l)
        if args.setup:
            # poll instead of sleeping: a fresh world needs a variable time to load its first chunk
            deadline = time.time() + args.settle
            # every `forceload add X Z` in the setup must have loaded (X Z are block coordinates)
            spots = [(0, 0)] + [(int(m.group(1)), int(m.group(2))) for m in (re.match(r"forceload add (-?\d+) (-?\d+)$", c) for c in args.setup) if m]
            for x, z in dict.fromkeys(spots):
                while time.time() < deadline:
                    if any("Test passed" in l for l in srv.command(f"execute if loaded {x} 64 {z}")):
                        break
                    time.sleep(0.5)
                else:
                    problems.append(f"the chunk at {x} {z} did not become entity-ticking after --setup")
            time.sleep(0.5)
        for fn in args.run:
            print(f"== function {fn} ==")
            for l in srv.command(f"function {fn}"):
                print(l)
                if PARSE_ERRORS.search(l) or "ERROR" in l:
                    problems.append(l)
        if args.checks:
            print("== checks ==")
            with open(args.checks) as cf:
                for chk in json.load(cf):
                    if chk.get("wait"):
                        time.sleep(chk["wait"])  # let game ticks pass (for #[tick] / #[every])
                    out = " ".join(srv.command(chk["cmd"]))
                    if "matches" in chk:
                        ok = re.search(chk["matches"], out) is not None
                        want = "/" + chk["matches"] + "/"
                    elif "absent" in chk:
                        ok = chk["absent"] not in out
                        want = "output without " + repr(chk["absent"])
                    else:
                        ok = chk.get("expect", "") in out
                        want = repr(chk.get("expect", ""))
                    print(("ok   " if ok else "FAIL ") + chk["cmd"] + "  ->  " + out.strip())
                    if not ok:
                        problems.append(f"check failed: `{chk['cmd']}` expected {want}, got {out.strip()!r}")
        srv.stop()
    finally:
        if args.keep:
            print(f"server dir kept: {run_dir}")
        else:
            shutil.rmtree(run_dir, ignore_errors=True)

    if problems:
        print("\n== PROBLEMS ==")
        for p in problems:
            print(p)
        sys.exit(1)
    print("\nOK: the game accepted the pack")


if __name__ == "__main__":
    main()
