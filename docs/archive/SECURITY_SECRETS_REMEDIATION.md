# Secrets Remediation Procedure

This repository no longer tracks `.env` files. If historical commits contained secrets, complete this remediation sequence before the next release.

## 1. Rotate Credentials

- Rotate all previously exposed credentials in the backing systems.
- Revoke all old JWT and admin passwords.

## 2. Rewrite Git History

Use `git filter-repo` (or BFG) in a maintenance window:

```bash
git filter-repo --path .env --invert-paths
```

Then force-push protected branches with maintainer coordination.

## 3. Re-scan History

- Run repository-wide gitleaks scan.
- Verify CI secret scan passes.
- Verify `.env` is ignored and only `.env.example` is tracked.

## 4. Enforce Ongoing Controls

- Install pre-commit hooks.
- Keep secret scanning required in branch protection.

```bash
pip install pre-commit
pre-commit install
pre-commit run --all-files
```
