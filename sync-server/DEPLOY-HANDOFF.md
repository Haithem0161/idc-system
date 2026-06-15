# IDC VPS Deploy -- Handoff (things only you can do)

Server-side setup is complete and verified on the VPS (149.102.139.41) on 2026-06-15.
This file lists the steps that need YOUR machine (signing key / GitHub login).

> Secrets generated on the VPS were printed once in the deploy session. They live
> in `sync-server/.env` (chmod 600, gitignored) and `sync-server/jwt_*.pem`. Keep
> the copies you saved; they are not reprinted here.

---

## 1. Generate the CI deploy SSH keypair (on your machine)

```bash
ssh-keygen -t ed25519 -C "idc-ci-deploy" -f ~/.ssh/idc-ci-deploy -N ""
```

- Send me (or paste into the VPS yourself) the PUBLIC key: `~/.ssh/idc-ci-deploy.pub`.
  I will drop it into `/home/idcdeploy/.ssh/authorized_keys` behind the validated
  forced command. If you do it yourself, the exact line is:

  ```
  restrict,command="/usr/bin/rrsync -munge -wo /var/www/idc-updates" <CONTENTS OF idc-ci-deploy.pub>
  ```

  (chmod 600, owned idcdeploy:idcdeploy. The forced command is validated:
  write-only, docroot-confined, no shell, no read, no `..` traversal.)

- The PRIVATE key `~/.ssh/idc-ci-deploy` becomes the `DEPLOY_SSH_KEY` GitHub secret.

## 2. Point the desktop app's updater at the real releases host + commit

Edit `src-tauri/tauri.conf.json` line 53:

```
-  "https://RELEASES_HOST_TODO.invalid/idc/{{target}}/{{arch}}/latest.json"
+  "https://idc-release.madebyhaithem.com/idc/{{target}}/{{arch}}/latest.json"
```

CI HARD-FAILS if the placeholder remains, and requires this host to equal the
`UPDATE_HOST` secret. The updater `pubkey` (minisign) is already set -- do not change it.
Commit this change before tagging a release.

## 3. Sync base URL (no change needed)

The desktop app already defaults the sync server URL to
`https://idc-sync.madebyhaithem.com`:
- `src/components/setup/first-launch-setup.tsx:11`
- `src/pages/auth/first-run.tsx:13`
(overridable via `VITE_SYNC_SERVER_URL`). Nothing to edit unless you want a different host.

## 4. GitHub Actions secrets (release.yml consumes exactly these 8)

Fill from your machine:

```bash
gh secret set UPDATE_HOST        --body "idc-release.madebyhaithem.com"
gh secret set DEPLOY_SSH_USER    --body "idcdeploy"
gh secret set DEPLOY_SSH_HOST    --body "149.102.139.41"
gh secret set DEPLOY_DOCROOT     --body "/var/www/idc-updates"
gh secret set DEPLOY_SSH_PORT    --body "22"

# DEPLOY_KNOWN_HOSTS -- pin the VPS host key (ed25519 line, verified authentic):
gh secret set DEPLOY_KNOWN_HOSTS --body "149.102.139.41 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMMovcD+dLp9S9HiR7xoMRNwFQT7u04oANC3v6amUfuG"

# From your machine (Step 1 + your Tauri signing key):
gh secret set DEPLOY_SSH_KEY              < ~/.ssh/idc-ci-deploy        # the PRIVATE key
gh secret set TAURI_SIGNING_PRIVATE_KEY   --body "<your Tauri/minisign signing private key>"
```

Host-key fingerprint (for your own cross-check): `SHA256:7rQzGODTOMG30a6KjKrmgpbwBk0GKgB6hmo0F+3G6/c` (ED25519).

---

## Operational note: login is entity-scoped

The sync server scopes login by `entityId`. The bootstrap superadmin lives under
tenant `3627804e-3594-4d6f-9e8c-b157e460e7f4` (BOOTSTRAP_TENANT_ID). A login without
the matching `entityId` returns 401 "invalid credentials" (NOT a signing error).
Verified working: `POST /auth/login` with that entityId returns a 200 + RS256 JWT.
