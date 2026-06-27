#!/usr/bin/env python3
"""Verify Hugging Face model availability and downloadable files.

Usage:
  python scripts/verify_hf_models.py
  python scripts/verify_hf_models.py --repo sshleifer/tiny-gpt2 --file config.json
  python scripts/verify_hf_models.py --repo sshleifer/tiny-gpt2 --repo hf-internal-testing/tiny-random-bert --file config.json
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from huggingface_hub import hf_hub_download, list_repo_files


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify model repositories and file downloads from Hugging Face Hub."
    )
    parser.add_argument(
        "--repo",
        action="append",
        dest="repos",
        default=[],
        help="Model repo id to validate. Repeatable.",
    )
    parser.add_argument(
        "--file",
        default="config.json",
        help="File expected in each repository and downloaded for verification.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repos = args.repos or ["sshleifer/tiny-gpt2", "hf-internal-testing/tiny-random-bert"]
    filename = args.file

    print(f"Checking {len(repos)} Hugging Face repos for file '{filename}'")

    failures = 0
    for repo in repos:
        print(f"\\nRepo: {repo}")
        try:
            files = list_repo_files(repo)
            listed = filename in files
            print(f"  list_repo_files: ok (files={len(files)}, has_target={listed})")
            if not listed:
                print(f"  error: '{filename}' not listed in repository")
                failures += 1
                continue

            downloaded = hf_hub_download(repo_id=repo, filename=filename)
            exists = Path(downloaded).exists()
            print(f"  hf_hub_download: ok (exists={exists})")
            print(f"  local_path: {downloaded}")
            if not exists:
                failures += 1
        except Exception as exc:  # noqa: BLE001
            failures += 1
            print(f"  error: {exc}")

    if failures:
        print(f"\\nFAILED: {failures} repository checks failed")
        return 1

    print("\\nOK: all repositories passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
