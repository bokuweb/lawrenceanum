"""Compare the pinned Wiki revision and case set with the actual Pages export."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import time
import urllib.error
import urllib.request


def case_digest(index):
    paths = sorted(p["path"] for p in index["pages"] if p["type"] == "hanrei")
    return hashlib.sha256("\n".join(paths).encode()).hexdigest()


def read_public(base_url):
    # Cache-busting avoids mistaking a previously cached deployment for the current one.
    for suffix in (".gz", ""):
        url = base_url.rstrip("/") + "/wiki/index.json" + suffix + "?check=" + str(time.time_ns())
        try:
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read()
            if data.startswith(b"\x1f\x8b"):
                data = gzip.decompress(data)
            return json.loads(data)
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
    return {}


def needs_update(index, expected_sha):
    return index.get("source_commit") != expected_sha


def verify(index, sha, digest):
    if needs_update(index, sha):
        raise ValueError("Published Wiki commit does not match the pinned revision")
    if case_digest(index) != digest:
        raise ValueError("Published case IDs do not match the exported Wiki")


def publication_watch(index, sha, previous, now):
    if not needs_update(index, sha):
        return {}, False
    since = previous.get("unpublished_since", now)
    return {"unpublished_since": since, "latest_wiki_sha": sha}, now - since >= 24 * 60 * 60


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("detect", "digest", "verify", "watch"))
    parser.add_argument("--base-url")
    parser.add_argument("--sha")
    parser.add_argument("--case-digest")
    parser.add_argument("--index")
    parser.add_argument("--watch-file")
    args = parser.parse_args()
    if args.mode == "digest":
        with open(args.index) as stream:
            print(case_digest(json.load(stream)))
        return
    if args.mode == "watch":
        path = Path(args.watch_file)
        previous = json.loads(path.read_text()) if path.exists() else {}
        state, overdue = publication_watch(read_public(args.base_url), args.sha, previous, time.time())
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(state, indent=2) + "\n")
        if overdue:
            raise ValueError("Wiki changes have remained unpublished for at least 24 hours")
        return
    if args.mode == "detect":
        print("changed=" + str(needs_update(read_public(args.base_url), args.sha)).lower())
        return
    for attempt in range(6):
        try:
            verify(read_public(args.base_url), args.sha, args.case_digest)
            print("Published Wiki revision and case IDs verified")
            return
        except (ValueError, urllib.error.URLError) as error:
            if attempt == 5:
                raise
            print(f"Waiting for Pages: {error}", flush=True)
            time.sleep(5)


if __name__ == "__main__":
    main()
