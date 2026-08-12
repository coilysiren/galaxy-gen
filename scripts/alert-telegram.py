#!/usr/bin/env python3
"""Send the CI or CD failure alert to Telegram.

Stdlib-only so the runner needs no package installation. Every workflow calls
this instead of carrying its own copy of the body. Kept byte-compatible with
coilyco-bridge/deploy's copy so the two can converge on one source later.

A workflow passes only what the runner cannot supply: BOT_TOKEN and CHAT_ID.
Everything else falls back to the runner's own GITHUB_* variables, and any
explicit value still wins. Retiring the two secrets needs the Ward
/api/create_alert path, which is gated on deploy#339.
"""

from __future__ import annotations

import os
import sys
import urllib.error
import urllib.parse
import urllib.request


def field(name: str, *runner_vars: str) -> str:
    """An explicit override, else the runner's own value, else a visible gap.

    A missing field must never cost the alert. "job: ?" still tells someone
    which workflow broke.
    """
    for var in (name, *runner_vars):
        value = os.environ.get(var, "").strip()
        if value:
            return value
    return "?"


def run_url() -> str:
    explicit = os.environ.get("RUN_URL", "").strip()
    if explicit:
        return explicit
    server = os.environ.get("GITHUB_SERVER_URL", "").rstrip("/")
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    if not (server and repo and run_id):
        return "?"
    return f"{server}/{repo}/actions/runs/{run_id}"


def build_message() -> str:
    return "\n".join(
        [
            f"{field('ALERT_KIND') if os.environ.get('ALERT_KIND') else 'CI'} failed on main",
            f"repo: {field('REPO', 'GITHUB_REPOSITORY')}",
            f"workflow: {field('WORKFLOW', 'GITHUB_WORKFLOW')}",
            f"job: {field('JOB', 'GITHUB_JOB')}",
            f"ref: {field('REF', 'GITHUB_REF')}",
            f"sha: {field('SHA', 'GITHUB_SHA')}",
            f"run: {run_url()}",
        ]
    )


def main() -> int:
    bot_token = os.environ.get("BOT_TOKEN", "")
    chat_id = os.environ.get("CHAT_ID", "")
    if not bot_token or not chat_id:
        print("telegram alert missing required secret", file=sys.stderr)
        return 2

    payload = urllib.parse.urlencode(
        {
            "chat_id": chat_id,
            "text": build_message(),
            "disable_web_page_preview": "true",
        }
    ).encode("utf-8")
    api_base = os.environ.get("API_BASE", "https://api.telegram.org").rstrip("/")
    request = urllib.request.Request(
        f"{api_base}/bot{bot_token}/sendMessage",
        data=payload,
        method="POST",
    )

    proxy_url = os.environ.get("FORGEJO_EGRESS_PROXY", "").strip()
    opener = urllib.request.build_opener(
        urllib.request.ProxyHandler({"https": proxy_url})
        if proxy_url
        else urllib.request.ProxyHandler()
    )
    try:
        with opener.open(request, timeout=15) as response:
            response.read()
    except urllib.error.URLError as exc:
        print(f"telegram alert failed ({type(exc).__name__})", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
